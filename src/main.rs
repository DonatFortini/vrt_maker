use std::process::Command;

fn build_ortho_vrt() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("gdalbuildvrt -resolution highest mosaic.vrt data/jp2/*.jp2")
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        eprintln!("Command failed with status: {}", output.status);
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    } else {
        println!("Orthophoto VRT created successfully");
    }
}

fn build_dem_vrt() -> bool {
    let output = Command::new("sh")
        .arg("-c")
        .arg("gdalbuildvrt -resolution highest temp_dem.vrt data/asc/*.asc")
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        eprintln!("Initial DEM VRT creation failed: {}", output.status);
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        return false;
    }
    println!("Initial DEM VRT created");

    let output = Command::new("sh")
        .arg("-c")
        .arg("gdal_fillnodata -md 200 -si 1 temp_dem.vrt temp_filled_dem.vrt")
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        eprintln!("DEM hole filling failed: {}", output.status);
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        let _ = std::fs::remove_file("temp_dem.vrt");
        return false;
    }
    println!("Holes filled in DEM");

    let output = Command::new("sh")
        .arg("-c")
        .arg("gdalwarp -tr 0.2 0.2 -r cubicspline -dstnodata 0 -wo NUM_THREADS=ALL_CPUS temp_filled_dem.vrt dem.vrt")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        println!("DEM VRT resampled successfully");
        true
    } else {
        eprintln!("DEM resampling failed: {}", output.status);
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        false
    }
}

fn resize_and_convert() {
    let commands = [
        "gdal_translate -of GTiff mosaic.vrt orthophoto.tiff",
        "gdal_translate -of GTiff -a_nodata 0 dem.vrt dem.tiff",
    ];

    for command in commands.iter() {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .expect("Failed to execute command");

        if !output.status.success() {
            eprintln!(
                "Command '{}' failed with status: {}",
                command, output.status
            );
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        } else {
            println!("Command '{}' executed successfully", command);
        }
    }
}

fn main() {
    build_ortho_vrt();
    if !build_dem_vrt() {
        eprintln!("Failed to process DEM VRT");
        return;
    }
    resize_and_convert();
}
