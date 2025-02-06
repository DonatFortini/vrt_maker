use std::fs;
use std::path::Path;
use std::process::Command;

fn ensure_directories() -> std::io::Result<()> {
    fs::create_dir_all("out/tmp")?;
    Ok(())
}

fn cleanup_vrts() -> std::io::Result<()> {
    let tmp_dir = Path::new("out/tmp");
    if tmp_dir.exists() {
        for entry in fs::read_dir(tmp_dir)? {
            let entry = entry?;
            if entry.path().extension().unwrap_or_default() == "vrt" {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn build_ortho_vrt() -> bool {
    Command::new("sh")
        .arg("-c")
        .arg("gdalbuildvrt -resolution highest out/tmp/mosaic.vrt data/jp2/*.jp2")
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn build_dem_vrt() -> bool {
    let commands = [
        "gdalbuildvrt -resolution highest out/tmp/temp_dem.vrt data/asc/*.asc",
        "gdal_fillnodata -md 200 -si 1 out/tmp/temp_dem.vrt out/tmp/temp_filled_dem.vrt",
        "gdalwarp -tr 0.2 0.2 -r cubicspline -dstnodata 0 -wo NUM_THREADS=ALL_CPUS out/tmp/temp_filled_dem.vrt out/tmp/dem.vrt"
    ];

    for cmd in &commands {
        let success = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        if !success {
            eprintln!("Command failed: {}", cmd);
            return false;
        }
    }
    true
}

fn resize_and_convert() -> bool {
    let commands = [
        "gdal_translate -of GTiff out/tmp/mosaic.vrt out/orthophoto.tiff",
        "gdal_translate -of GTiff -a_nodata 0 out/tmp/dem.vrt out/dem.tiff",
    ];

    for cmd in &commands {
        let success = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        if !success {
            eprintln!("Command failed: {}", cmd);
            return false;
        }
    }
    true
}

fn main() {
    if let Err(e) = ensure_directories() {
        eprintln!("Failed to create directories: {}", e);
        return;
    }

    if let Err(e) = cleanup_vrts() {
        eprintln!("Failed to cleanup VRTs: {}", e);
        return;
    }

    if !build_ortho_vrt() {
        eprintln!("Failed to build orthophoto VRT");
        return;
    }

    if !build_dem_vrt() {
        eprintln!("Failed to process DEM VRT");
        return;
    }

    if !resize_and_convert() {
        eprintln!("Failed to create final TIFFs");
    }
}
