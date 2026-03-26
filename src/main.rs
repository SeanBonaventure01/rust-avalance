fn build_open_topo_get_request(west_lat: f32, east_lat: f32, north_long: f32, south_long: f32, api_key: &str) -> String
{
   format!("https://portal.opentopography.org/API/usgsdem?datasetName=USGS10m&\
       south={}&north={}&west={}&east={}&outputFormat=GTiff&API_Key={}", south_long, north_long, west_lat, east_lat, api_key) 
}

fn get_utm_zone_from_longitude(longitude: f32) -> i32
{
    let deg_per_utm = 6.0;
    let adj_lon = if longitude < 0.0 {longitude + 360.0} else {longitude};
    (((adj_lon - 180.0) / deg_per_utm + 1.0) as i32).abs()
}

fn get_epsg_utm_str_from_lat_long((lon, lat): (f32, f32)) -> String
{
    let utm_zone = get_utm_zone_from_longitude(lon);
    let north_code = if lat > 90.0 {7} else {6};
    format!("EPSG:32{north_code}{:02}", utm_zone)
}

fn compute_slope_angle(elevation_vec: &Vec<f32>, x_dim: usize, y_dim: usize) -> Vec<f32>
{
    vec![0.0; x_dim * y_dim]
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
    let (width, height) = gdal_dataset.raster_size();
    let Ok(geo_transform) = gdal_dataset.geo_transform() else {
        println!("Couldn't get geo transform!");
        return;
    };

    println!("Upperleft (x, y): ({}, {}). W-E resolution: {}. N-S resolution: {}", geo_transform[0], geo_transform[3], geo_transform[1], geo_transform[5]);

    let Ok(raster) = gdal_dataset.rasterband(1) else {
        println!("Couldn't fetch raster");
        return;
    };
    let (raster_cols, raster_rows) = raster.size();
    let mut values: Vec<f32> = vec![0.0; raster_cols * raster_rows];
    let Ok(_) = raster.read_into_slice::<f32>((0, 0), raster.size(), (raster_cols, raster_rows), values.as_mut_slice(), None) else {
        println!("Failed to get values into vector!");
        return;
    };

    //let slop_angle = compute_slope_angle(&values, raster_cols, raster_rows);
    let mut slope_opts = gdal::raster::processing::dem::SlopeOptions::new();
    slope_opts.with_algorithm(gdal::raster::processing::dem::DemSlopeAlg::Horn).with_scale(111120.0);
    let Ok(slope_ds) = gdal::raster::processing::dem::slope(&gdal_dataset, std::path::Path::new("slope-angle.tiff"), &slope_opts) else {
        println!("Couldn't convert to slope!");
        return;
    };

    let lat_long_test: (f32, f32) = ( -122.2, 47.6);
    let utm_str = get_epsg_utm_str_from_lat_long(lat_long_test);
    // Convert from WGS84 (lat/lon) to UTM. EPSG codes are the standard defintions of each coord
    // system
    let proj_transform = match proj::Proj::new_known_crs("EPSG:4326", &utm_str, None) {
        Ok(v) => v,
        Err(e) => {
            println!("Couldn't create projection! Error: {}", e);
            return;
        }
    };

    let utm_coords = match proj_transform.project(lat_long_test, false) {
        Ok(v) => v,
        Err(e) => {
            println!("Failed to convert! Error : {}", e);
            return;
        }
    };
    println!("Seattle UTM Coords: {:?}", utm_coords);
}
