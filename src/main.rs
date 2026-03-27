use geo::{Haversine, Distance};

fn build_open_topo_get_request(west_lat: f32, east_lat: f32, north_long: f32, south_long: f32, api_key: &str) -> String
{
   format!("https://portal.opentopography.org/API/usgsdem?datasetName=USGS10m&\
       south={}&north={}&west={}&east={}&outputFormat=GTiff&API_Key={}", south_long, north_long, west_lat, east_lat, api_key) 
}

fn get_utm_zone_from_longitude(longitude: f64) -> i32
{
    let deg_per_utm = 6.0;
    let adj_lon = if longitude < 0.0 {longitude + 360.0} else {longitude};
    (((adj_lon - 180.0) / deg_per_utm + 1.0) as i32).abs()
}

fn get_epsg_utm_str_from_lat_long((lon, lat): (f64, f64)) -> String
{
    let utm_zone = get_utm_zone_from_longitude(lon);
    let north_code = if lat > 90.0 {7} else {6};
    format!("EPSG:32{north_code}{:02}", utm_zone)
}

fn unwrap_weight(val: Option<f32>) -> f32
{
    match val {
        Some(v) => 1.0,
        None => 0.0
    }
}

fn extract_elevation(val: Option<f32>) -> f32
{
    match val {
        Some(v) => v,
        None => 0.0
    }
}

// We take in an option because on the edges the cells are not full
fn compute_single_slope_angle(input_data: &[Option<f32>; 9], (x_res_m, y_res_m) : (f64, f64)) -> f32 
{
    // See https://pro.arcgis.com/en/pro-app/3.4/tool-reference/spatial-analyst/how-slope-works.htm
    // on how this equation works
    let wght1 = unwrap_weight(input_data[2]) + 2.0*unwrap_weight(input_data[5]) + unwrap_weight(input_data[8]);
    let wght2 = unwrap_weight(input_data[0]) + 2.0*unwrap_weight(input_data[3]) + unwrap_weight(input_data[6]);
    // I think technically wght 3 and 4 are swapped according to the equation but lets just go with
    // it
    let wght3 = unwrap_weight(input_data[0]) + 2.0*unwrap_weight(input_data[1]) + unwrap_weight(input_data[2]);
    let wght4 = unwrap_weight(input_data[6]) + 2.0*unwrap_weight(input_data[7]) + unwrap_weight(input_data[8]);
    let x_slope_angle = ((extract_elevation(input_data[2]) + 2.0*extract_elevation(input_data[5]) + extract_elevation(input_data[8])*4.0)/(if wght1 > 0.0 {wght1} else {1.0})
        - ((extract_elevation(input_data[0]) + 2.0*extract_elevation(input_data[3]) + extract_elevation(input_data[6])*4.0)/(if wght2 > 0.0 {wght2} else {0.0}))) / (8.0 * x_res_m as f32);
    let y_slope_angle = ((extract_elevation(input_data[6]) + 2.0*extract_elevation(input_data[7]) + extract_elevation(input_data[8])*4.0)/(if wght4 > 0.0 {wght4} else {1.0})
        - ((extract_elevation(input_data[0]) + 2.0*extract_elevation(input_data[1]) + extract_elevation(input_data[2])*4.0)/(if wght3 > 0.0 {wght3} else {1.0}))) / (8.0 * y_res_m as f32);
    ((x_slope_angle*x_slope_angle + y_slope_angle*y_slope_angle).sqrt()).atan() * 57.29578
 
}

fn compute_slope_angle_from_vector(input_elevations: &Vec<f32>, (x_size, y_size): (usize, usize), 
    (x_res_m, y_res_m) : (f64, f64)) -> Result<Vec<f32>, Box<dyn std::error::Error>>
{
    println!("Input elevation: {}", input_elevations[0]);
    let mut output_slope_angle = vec![0.0; x_size*y_size];
    for y in 0..y_size {
        for x in 0..x_size {
            let mut points: Vec<Option<f32>> = vec![None; 9];
            let target_index = x + (x_size*y);
            points[0] = if x != 0 && y != 0 {Some(input_elevations[target_index - 1 - x_size])} else {None};
            points[1] = if y != 0 {Some(input_elevations[target_index - x_size])} else {None};
            points[2] = if x != (x_size - 1) && y != 0 {Some(input_elevations[target_index + 1 - x_size])} else {None};
            points[3] = if x != 0 {Some(input_elevations[target_index - 1])} else {None};
            points[4] = Some(input_elevations[target_index]);
            points[5] = if x != (x_size - 1) {Some(input_elevations[target_index + 1])} else {None};
            points[6] = if x != 0 && y != (y_size - 1) {Some(input_elevations[target_index - 1 + x_size])} else {None};
            points[7] = if y != (y_size - 1) {Some(input_elevations[target_index + x_size])} else {None};
            points[8] = if x != (x_size - 1) && y != (y_size - 1) {Some(input_elevations[target_index + 1 + x_size])} else {None};
            // For now skip any that aren't full
            if x > 0 && x < x_size - 1 && y > 0 && y < y_size - 1
            {
                output_slope_angle[target_index] = compute_single_slope_angle(points.as_slice().try_into().unwrap(), (x_res_m, y_res_m));
            }
            else
            {
                output_slope_angle[target_index] = 0.0;
            }
        }
    }
    println!("({x_size}, {y_size})");
    Ok(output_slope_angle)
}

fn compute_slope_angle_from_dataset(slope_dataset: &gdal::Dataset) -> Result<Vec<f32>, Box<dyn std::error::Error>>
{
    // 1. Read into vector of elevations
    let Ok(raster) = slope_dataset.rasterband(1) else {
        return Err("Couldn't fetch raster".into());
    };

    let (raster_cols, raster_rows) = raster.size();
    let mut values: Vec<f32> = vec![0.0; raster_cols * raster_rows];
    let Ok(_) = raster.read_into_slice::<f32>((0, 0), raster.size(), (raster_cols, raster_rows), values.as_mut_slice(), None) else {
        return Err("Couldn't read into slice".into());
    };

    // 2. Determine pixel resolution in meters. Get the distance in meters across the image for x
    //    res. Y is a constant 111,111m/deg 
    let Ok(geo_transform) = slope_dataset.geo_transform() else {
        return Err("Couldn't get geo transform!".into());
    };

    let mut coords: [(f64, f64); 2] = [(geo_transform[0], geo_transform[3]), (geo_transform[0] + (geo_transform[1]*(raster_cols as f64)), geo_transform[3])];
    let hav_distance = Haversine.distance(coords[0].into(), coords[1].into());
    // Lattitude is a constant 111,111m/degree
    let (x_res, y_res) : (f64, f64) = (hav_distance/(raster_cols as f64), -1.0 * geo_transform[5] * 111111.0);

    return compute_slope_angle_from_vector(&values, (raster_cols, raster_rows), (x_res, y_res));
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

    //let slop_angle = compute_slope_angle(&values, raster_cols, raster_rows);
    let mut slope_opts = gdal::raster::processing::dem::SlopeOptions::new();
    slope_opts.with_algorithm(gdal::raster::processing::dem::DemSlopeAlg::Horn).with_scale(111120.0);
    let Ok(slope_ds) = gdal::raster::processing::dem::slope(&gdal_dataset, std::path::Path::new("slope-angle.tiff"), &slope_opts) else {
        println!("Couldn't convert to slope!");
        return;
    };

    let slope_angles = match compute_slope_angle_from_dataset(&gdal_dataset) {
        Ok(v) => v,
        Err(e) => {
            println!("Couldn't calculate slope angle: {e}");
            return;
        }
    };
    println!("Computed slope angles! First few values:");
    for i in 866..876
    {
        println!("{}", slope_angles[i]);
    }
}
