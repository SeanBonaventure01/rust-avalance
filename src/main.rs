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
    // For now we assume we are only working on data with all valid cells (ie, non edges)
    let wght1 = 4.0; //unwrap_weight(input_data[2]) + 2.0*unwrap_weight(input_data[5]) + unwrap_weight(input_data[8]);
    let wght2 = 4.0; //unwrap_weight(input_data[0]) + 2.0*unwrap_weight(input_data[3]) + unwrap_weight(input_data[6]);
    // I think technically wght 3 and 4 are swapped according to the equation but lets just go with
    // it
    let wght3 = 4.0; //unwrap_weight(input_data[0]) + 2.0*unwrap_weight(input_data[1]) + unwrap_weight(input_data[2]);
    let wght4 = 4.0; //unwrap_weight(input_data[6]) + 2.0*unwrap_weight(input_data[7]) + unwrap_weight(input_data[8]);
    let x_slope_angle = (((extract_elevation(input_data[2]) + 2.0*extract_elevation(input_data[5]) + extract_elevation(input_data[8]))*4.0)/(if wght1 > 0.0 {wght1} else {1.0})
        - ((extract_elevation(input_data[0]) + 2.0*extract_elevation(input_data[3]) + extract_elevation(input_data[6]))*4.0)/(if wght2 > 0.0 {wght2} else {0.0})) / (8.0 * x_res_m as f32);
    let y_slope_angle = (((extract_elevation(input_data[6]) + 2.0*extract_elevation(input_data[7]) + extract_elevation(input_data[8]))*4.0)/(if wght4 > 0.0 {wght4} else {1.0})
        - ((extract_elevation(input_data[0]) + 2.0*extract_elevation(input_data[1]) + extract_elevation(input_data[2]))*4.0)/(if wght3 > 0.0 {wght3} else {1.0})) / (8.0 * y_res_m as f32);
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

fn convert_slope_vector_to_dataset(original_dataset: &gdal::Dataset, slope_data: Vec<f32>) -> Result<gdal::Dataset, Box<dyn std::error::Error>>
{
    // MEM driver just creates a dataset in memory as opposed to GTiff which requires a file
    let driver = gdal::DriverManager::get_driver_by_name("MEM")?;
    // Get the original datasets metadata that we need to copy to the new dataset
    let og_projection = original_dataset.projection();
    let og_spatial_ref = original_dataset.spatial_ref()?;
    let og_geo_transform = original_dataset.geo_transform()?;
    let (original_x, original_y) = original_dataset.rasterband(1)?.size();
    let mut new_dataset = driver.create_with_band_type::<f32, _>("", original_x, original_y, 1)?;
    let _ = new_dataset.set_geo_transform(&og_geo_transform)?;
    let _ = new_dataset.set_spatial_ref(&og_spatial_ref)?;
    let _ = new_dataset.set_projection(&og_projection)?;

    let mut rasterband = new_dataset.rasterband(1)?;
    let mut buff = gdal::raster::Buffer::new((original_x, original_y), slope_data);
    rasterband.write((0, 0), (original_x, original_y), &mut buff);
    let _ = new_dataset.flush_cache();

    Ok(new_dataset)
}

fn save_slope_to_file(slope_dataset: &gdal::Dataset, file_path: &str) -> Result<(), Box<dyn std::error::Error>>
{
    let driver = gdal::DriverManager::get_driver_by_name("GTiff")?;
    let creation_options = gdal::raster::RasterCreationOptions::new();
    let mut gtiff_dataset = slope_dataset.create_copy(&driver, std::path::Path::new(file_path), &creation_options)?;
    let _ = gtiff_dataset.flush_cache();
    Ok(())
}

fn get_slope_angle_from_point(slope_dataset: &gdal::Dataset, (lat, lon) : (f64, f64)) -> Result<f32, Box<dyn std::error::Error>>
{
    let raster_band = slope_dataset.rasterband(1)?;
    let (x_size, y_size) = raster_band.size();
    let geo_transform = slope_dataset.geo_transform()?;
    let (x_index, y_index) = (((lon - geo_transform[0])/geo_transform[1]) as isize, ((lat - geo_transform[3])/geo_transform[5]) as isize);
    let mut buff = vec![0.0; 1];
    let _ = raster_band.read_into_slice((x_index, y_index), (1, 1), (1, 1), &mut buff, None)?;
    Ok(buff[0])
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
    // let (x_res, y_res) = (1.0 * geo_transform[1] * 111111.0, -1.0 * geo_transform[5] * 111111.0);
    let (x_res, y_res) : (f64, f64) = (hav_distance/(raster_cols as f64), -1.0 * geo_transform[5] * 111111.0);
    println!("Xres: {x_res}, yres: {y_res}");

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

    let mut slope_opts = gdal::raster::processing::dem::SlopeOptions::new();
    slope_opts.with_algorithm(gdal::raster::processing::dem::DemSlopeAlg::Horn).with_scale(111120.0);
    let Ok(slope_ds) = gdal::raster::processing::dem::slope(&gdal_dataset, std::path::Path::new("slope-angle.tif"), &slope_opts) else {
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
    
    let Ok(slope_dataset) = convert_slope_vector_to_dataset(&gdal_dataset, slope_angles) else {
        println!("Unable to make new dataset!");
        return;
    };

    let _ = save_slope_to_file(&slope_dataset, "manual_slope_angles.tif");

    let (lat, long): (f64, f64) = (46.18260, -122.18900);
    let Ok(gdal_slope_angle) = get_slope_angle_from_point(&slope_ds, (lat, long)) else {
        println!("Unable to get slope angle for gdal dataset");
        return;
    };
    let Ok(manual_slope_angle) = get_slope_angle_from_point(&slope_dataset, (lat, long)) else {
        println!("Unable to get slope angle for manual dataset");
        return;
    };
    println!("Gdal slope angle: {gdal_slope_angle}, manual slope angle: {manual_slope_angle}");
}
