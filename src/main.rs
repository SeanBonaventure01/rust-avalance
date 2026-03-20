fn build_open_topo_get_request(west_lat: f32, east_lat: f32, north_long: f32, south_long: f32, api_key: &str) -> String
{
   format!("https://portal.opentopography.org/API/usgsdem?datasetName=USGS10m&\
       south={}&north={}&west={}&east={}&outputFormat=GTiff&API_Key={}", south_long, north_long, west_lat, east_lat, api_key) 
}

fn main() {
    let _ = dotenvy::dotenv();
    let Ok(open_topo_api_key) =  dotenvy::var("OPEN_TOPO_KEY") else {
        eprintln!("Please add an open topo API key to your .env!");
        return;
    };
    // Mount St Helens coordinates
    let request = build_open_topo_get_request(-122.22, -122.14, 46.22, 46.16, &open_topo_api_key);
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
    println!("Raster count: {}", gdal_dataset.raster_count());
    let (width, height) = gdal_dataset.raster_size();
    println!("Raster width: {width}, height: {height}");
}
