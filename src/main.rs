pub mod slope_angles;

fn main() {
    let _ = dotenvy::dotenv();
    let Ok(open_topo_api_key) =  dotenvy::var("OPEN_TOPO_KEY") else {
        eprintln!("Please add an open topo API key to your .env!");
        return;
    };
    // Mount St Helens coordinates
    let request = slope_angles::build_open_topo_get_request(-122.22, -122.14, 46.22, 46.16, &open_topo_api_key);
    println!("Making request for mt st helens...");
    let open_topo_req = reqwest::blocking::get(request);
    let Ok(response) = open_topo_req else {
        println!("Got an error return!");
        return;
    };

    println!("Response code: {}", response.status());

    let geo_tiff = match response.bytes() {
        Ok(b) => b,
        Err(e) => {
            println!("Unable to get bytes from response! Code: {}", e);
            return;
        }
    };

    println!("Byte len: {}", geo_tiff.len());
    println!("Writing to file...");
    let Ok(_) = std::fs::write("output.tif", &geo_tiff) else {
        println!("Failed to write file!");
        return;
    };

    let Ok(gdal_dataset) = gdal::Dataset::open("output.tif") else {
        println!("Failed to process dataset!");
        return;
    };

    let mut slope_opts = gdal::raster::processing::dem::SlopeOptions::new();
    slope_opts.with_algorithm(gdal::raster::processing::dem::DemSlopeAlg::Horn).with_scale(111120.0);
    let Ok(slope_ds) = gdal::raster::processing::dem::slope(&gdal_dataset, std::path::Path::new("slope-angle.tif"), &slope_opts) else {
        println!("Couldn't convert to slope!");
        return;
    };

    let slope_angles = match slope_angles::compute_slope_angle_from_dataset(&gdal_dataset) {
        Ok(v) => v,
        Err(e) => {
            println!("Couldn't calculate slope angle: {e}");
            return;
        }
    };
    
    let Ok(slope_dataset) = slope_angles::convert_slope_vector_to_dataset(&gdal_dataset, slope_angles) else {
        println!("Unable to make new dataset!");
        return;
    };
    let Ok(contours) = slope_angles::compute_contours_from_slope_angles(&slope_dataset) else {
        println!("Unable to get contours!");
        return;
    };

    let geometry = geojson::Geometry::from(&contours);
    let feature = geojson::Feature {
        geometry: Some(geometry),
        ..geojson::Feature::default()
    };

    // Wrap in a FeatureCollection if you have multiple
    let geojson = geojson::GeoJson::Feature(feature);
    
    // Serialize to string
    let geojson_string = geojson.to_string();
    
    // Write to file
    let _ = std::fs::write("output.geojson", geojson_string) else {
        println!("Failed to write to file!");
        return;
    };
}
