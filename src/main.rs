use clap::{Parser, Subcommand};
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const ZOOM_LEVEL: u32 = 19;
const PIXEL_SIZE: f64 = 0.2; // 20cm per pixel at zoom level 19
const TILE_SIZE: u32 = 256; // pixels

#[derive(Parser)]
#[command(name = "wmts-downloader")]
#[command(about = "Download tiles from IGN WMTS server with multi-threading support", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Get {
        /// Bounding box coordinates in Lambert 93 (minX,minY,maxX,maxY)
        #[arg(short, long, value_parser = parse_bbox)]
        bbox: (f64, f64, f64, f64),

        /// Output directory for downloaded tiles
        #[arg(short, long, default_value = "tiles")]
        output: PathBuf,

        /// Maximum concurrent downloads
        #[arg(short, long, default_value_t = 32)]
        concurrent: usize,

        /// Request timeout in milliseconds
        #[arg(short, long, default_value_t = 10000)]
        timeout: u64,
    },
}

#[derive(Debug, Clone)]
struct TileCoords {
    row: u32,
    col: u32,
    x: f64, // top-left corner X coordinate in Lambert 93
    y: f64, // top-left corner Y coordinate in Lambert 93
}

fn parse_bbox(s: &str) -> Result<(f64, f64, f64, f64), Box<dyn Error + Send + Sync>> {
    let coords: Vec<f64> = s
        .split(',')
        .map(|x| x.parse::<f64>())
        .collect::<Result<Vec<f64>, _>>()?;

    if coords.len() != 4 {
        return Err("bbox must have exactly 4 coordinates".into());
    }

    Ok((coords[0], coords[1], coords[2], coords[3]))
}

fn meters_to_tile(x: f64, y: f64) -> (u32, u32, f64, f64) {
    // Known working example:
    // Tile (195404,275651) is near coordinates (1223232.7321,6075925.1150)

    // Calculate distance from reference point
    let dx = x - 1223232.7321;
    let dy = y - 6075925.1150;

    // Base coordinates from known working tile
    let base_row = 195404;
    let base_col = 275651;

    // Each tile is 256x256 pixels at 0.2m resolution
    let tile_size_meters = TILE_SIZE as f64 * PIXEL_SIZE;

    // Calculate tile offsets (rounded to nearest tile)
    let col_offset = (dx / tile_size_meters).round() as i32;
    let row_offset = (-dy / tile_size_meters).round() as i32; // Negative because Y is inverted

    // Add offsets to base coordinates
    let col = (base_col as i32 + col_offset) as u32;
    let row = (base_row as i32 + row_offset) as u32;

    // Calculate actual top-left coordinates of the tile
    let tile_x = 1223232.7321 + (col_offset as f64 * tile_size_meters);
    let tile_y = 6075925.1150 - (row_offset as f64 * tile_size_meters);

    (row, col, tile_x, tile_y)
}

fn get_tile_coords(bbox: (f64, f64, f64, f64)) -> Vec<TileCoords> {
    let mut tiles = Vec::new();

    // Convert projected coordinates to tile coordinates
    let (row1, col1, _, _) = meters_to_tile(bbox.0, bbox.1);
    let (row2, col2, _, _) = meters_to_tile(bbox.2, bbox.3);

    // Ensure we have proper min/max values
    let (min_row, max_row) = (row2.min(row1), row2.max(row1));
    let (min_col, max_col) = (col1.min(col2), col1.max(col2));

    println!(
        "Tile ranges: Row {} to {}, Col {} to {}",
        min_row, max_row, min_col, max_col
    );

    for row in min_row..=max_row {
        for col in min_col..=max_col {
            let (_, _, x, y) = meters_to_tile(
                bbox.0 + (col - min_col) as f64 * TILE_SIZE as f64 * PIXEL_SIZE,
                bbox.1 + (row - min_row) as f64 * TILE_SIZE as f64 * PIXEL_SIZE,
            );
            tiles.push(TileCoords { row, col, x, y });
        }
    }

    tiles
}

async fn download_tile(
    client: &Client,
    coords: TileCoords,
    output_dir: &PathBuf,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let url = format!(
        "https://data.geopf.fr/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0\
        &LAYER=HR.ORTHOIMAGERY.ORTHOPHOTOS&STYLE=normal&FORMAT=image%2Fjpeg\
        &TILEMATRIXSET=PM_6_19&TILEMATRIX={}&TILEROW={}&TILECOL={}",
        ZOOM_LEVEL, coords.row, coords.col
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download tile {},{}: {}",
            coords.row,
            coords.col,
            response.status()
        )
        .into());
    }

    let content = response.bytes().await?;

    let filename = format!("tile_{}_{}.jpeg", coords.row, coords.col);
    let filepath = output_dir.join("tiles").join(filename);
    fs::write(filepath, content)?;

    Ok(())
}

fn create_vrt(
    tiles: &[TileCoords],
    output_dir: &PathBuf,
    downloaded_tiles: &AtomicUsize,
) -> Result<(), Box<dyn Error>> {
    // Skip VRT creation if no tiles were downloaded
    if downloaded_tiles.load(Ordering::Relaxed) == 0 {
        return Err("No tiles were successfully downloaded".into());
    }
    let vrt_path = output_dir.join("mosaic.vrt");
    let mut file = File::create(vrt_path)?;

    // Write VRT header
    writeln!(
        file,
        "<VRTDataset rasterXSize=\"{}\" rasterYSize=\"{}\">",
        tiles.len() as u32 * TILE_SIZE,
        tiles.len() as u32 * TILE_SIZE
    )?;

    // Write georeference information
    writeln!(file, "  <SRS>EPSG:2154</SRS>")?; // Lambert 93

    // Write VRT bands (RGB for JPEG)
    for band in 1..=3 {
        writeln!(
            file,
            "  <VRTRasterBand dataType=\"Byte\" band=\"{}\">",
            band
        )?;
        writeln!(
            file,
            "    <ColorInterp>{}</ColorInterp>",
            match band {
                1 => "Red",
                2 => "Green",
                3 => "Blue",
                _ => unreachable!(),
            }
        )?;

        // Add each tile as a source
        for tile in tiles {
            let filename = format!("tile_{}_{}.jpeg", tile.row, tile.col);
            writeln!(file, "    <SimpleSource>")?;
            writeln!(
                file,
                "      <SourceFilename relativeToVRT=\"1\">tiles/{}</SourceFilename>",
                filename
            )?;
            writeln!(file, "      <SourceBand>{}</SourceBand>", band)?;
            writeln!(
                file,
                "      <SrcRect xOff=\"0\" yOff=\"0\" xSize=\"{}\" ySize=\"{}\"/>",
                TILE_SIZE, TILE_SIZE
            )?;

            // Calculate destination rectangle based on tile coordinates
            let x_off = (tile.col as i64 * TILE_SIZE as i64) as i64;
            let y_off = (tile.row as i64 * TILE_SIZE as i64) as i64;
            writeln!(
                file,
                "      <DstRect xOff=\"{}\" yOff=\"{}\" xSize=\"{}\" ySize=\"{}\"/>",
                x_off, y_off, TILE_SIZE, TILE_SIZE
            )?;
            writeln!(file, "    </SimpleSource>")?;
        }
        writeln!(file, "  </VRTRasterBand>")?;
    }

    writeln!(file, "</VRTDataset>")?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    match args.command {
        Commands::Get {
            bbox,
            output,
            concurrent,
            timeout,
        } => {
            // Create output directory and tiles subdirectory if they don't exist
            let tiles_dir = output.join("tiles");
            fs::remove_dir_all(&tiles_dir).ok(); // Remove if exists
            fs::create_dir_all(&tiles_dir)?;

            // Setup HTTP client with timeout
            let client = Client::builder()
                .timeout(Duration::from_millis(timeout))
                .build()?;
            let client = Arc::new(client);

            // Create semaphore for concurrency control
            let semaphore = Arc::new(Semaphore::new(concurrent));

            // Get tile coordinates from bbox
            let tiles = get_tile_coords(bbox);
            let total_tiles = tiles.len();

            if total_tiles == 0 {
                println!("No tiles found in the specified bounding box!");
                return Ok(());
            }

            println!("Preparing to download {} tiles...", total_tiles);

            // Setup progress bar
            let progress_bar = ProgressBar::new(total_tiles as u64);
            progress_bar.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} tiles ({percent}%)",
                    )
                    .unwrap()
                    .progress_chars("=>-"),
            );

            // Create futures for all downloads
            let mut handles = vec![];

            for tile in tiles.clone() {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let client = client.clone();
                let output = output.clone();
                let progress_bar = progress_bar.clone();

                let handle = tokio::spawn(async move {
                    let result = download_tile(&client, tile, &output).await;
                    progress_bar.inc(1);
                    drop(permit);
                    result
                });

                handles.push(handle);
            }

            // Wait for all downloads to complete
            let results = join_all(handles).await;

            progress_bar.finish_with_message("Download completed");

            // Count successful and failed downloads
            let downloaded_tiles = Arc::new(AtomicUsize::new(0));
            let error_count = Arc::new(AtomicUsize::new(0));

            for result in results {
                match result {
                    Ok(Ok(_)) => {
                        downloaded_tiles.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(Err(e)) => {
                        error_count.fetch_add(1, Ordering::Relaxed);
                        eprintln!("Error downloading tile: {}", e);
                    }
                    Err(e) => {
                        error_count.fetch_add(1, Ordering::Relaxed);
                        eprintln!("Task error: {}", e);
                    }
                }
            }

            let success_count = downloaded_tiles.load(Ordering::Relaxed);
            let error_count = error_count.load(Ordering::Relaxed);

            println!("\nFinal results:");
            println!("Successfully downloaded: {} tiles", success_count);
            println!("Failed downloads: {} tiles", error_count);

            // Create VRT file if we have successful downloads
            println!("\nCreating VRT mosaic...");
            match create_vrt(&tiles, &output, &downloaded_tiles) {
                Ok(_) => println!("VRT file created successfully"),
                Err(e) => eprintln!("Error creating VRT file: {}", e),
            }
        }
    }

    Ok(())
}
