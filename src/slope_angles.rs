use std::io::Write;

mod slope_angle_helpers {
    use geo::{Haversine, Distance};

    pub fn build_open_topo_get_request(west_lon: f64, east_lon: f64, north_lat: f64, south_lat: f64, api_key: &str) -> String
    {
       format!("https://portal.opentopography.org/API/usgsdem?datasetName=USGS10m&\
           south={}&north={}&west={}&east={}&outputFormat=GTiff&API_Key={}", south_lat, north_lat, west_lon, east_lon, api_key) 
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
            Some(_) => 1.0,
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
    pub fn compute_single_slope_angle(input_data: &[Option<f32>; 9], (x_res_m, y_res_m) : (f64, f64)) -> f32 
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
    
    pub fn compute_slope_angle_from_vector(input_elevations: &Vec<f32>, (x_size, y_size): (usize, usize), 
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
    
    pub fn convert_slope_vector_to_dataset(original_dataset: &gdal::Dataset, slope_data: Vec<f32>) -> Result<gdal::Dataset, Box<dyn std::error::Error>>
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
    
    pub fn save_slope_to_file(slope_dataset: &gdal::Dataset, file_path: &str) -> Result<(), Box<dyn std::error::Error>>
    {
        let driver = gdal::DriverManager::get_driver_by_name("GTiff")?;
        let creation_options = gdal::raster::RasterCreationOptions::new();
        let mut gtiff_dataset = slope_dataset.create_copy(&driver, std::path::Path::new(file_path), &creation_options)?;
        let _ = gtiff_dataset.flush_cache();
        Ok(())
    }

    pub fn compute_slope_angle_from_dataset(slope_dataset: &gdal::Dataset) -> Result<Vec<f32>, Box<dyn std::error::Error>>
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

    pub fn compute_contours_from_slope_angles(slope_dataset: &gdal::Dataset) -> Result<geo_types::geometry::MultiPolygon<f64>, Box<dyn std::error::Error>>
    {
        let Ok(raster) = slope_dataset.rasterband(1) else {
            return Err("Couldn't fetch raster".into());
        };
    
        let (raster_cols, raster_rows) = raster.size();
        let mut values: Vec<f32> = vec![0.0; raster_cols * raster_rows];
        let Ok(_) = raster.read_into_slice::<f32>((0, 0), raster.size(), (raster_cols, raster_rows), values.as_mut_slice(), None) else {
            return Err("Couldn't read into slice".into());
        };
    
        let contour_builder = contour::ContourBuilder::new(raster_cols, raster_rows, true);
        // Todo: un hardcode avalanche value
        let slice: &[f64] = &values.iter().map(|&x| x as f64).collect::<Vec<f64>>();
        let contours = contour_builder.contours(&slice, &[30.0])?;
        // The contours are in arbitrary x/y units so we need to convert to lat/long using the dataset
        // Seems like we have to manually do this
        let geo_transform = slope_dataset.geo_transform()?;
        let (x_origin, y_origin, x_res, y_res) = (geo_transform[0], geo_transform[3], geo_transform[1], geo_transform[5]); 
        let projected_contours: geo_types::geometry::MultiPolygon = contours[0].geometry().iter().map(|polygon: &geo_types::geometry::Polygon<f64>| {
            // Exterior is what we really care about. A line containing the outer boundary of our
            // polygon
            let projected_exterior: geo_types::geometry::LineString<f64> = polygon.exterior().coords().map(|coord| geo_types::geometry::Coord {
                x : x_origin + (coord.x*x_res),
                y : y_origin + (coord.y*y_res),
            }).collect();
    
            // Interior is for inner holes
            // We need .iter() here becuse this just returns a slice. This gives us the actual
            // linestring we can then map the coords to
            let projected_interiors: Vec<geo_types::geometry::LineString<f64>> = polygon
                .interiors()
                .iter()
                .map(|interior| {
                    interior.coords().map(|coord| geo_types::geometry::Coord {
                        x : x_origin + (coord.x*x_res),
                        y : y_origin + (coord.y*y_res),
                    }).collect()
            }).collect();
            geo_types::geometry::Polygon::new(projected_exterior, projected_interiors) 
        }).collect();
    
        Ok(projected_contours)
    }
}

pub struct AvalancheTerrain {
    avalanche_dataset: gdal::Dataset,
    pub geo_json_out: geojson::Feature
}

impl AvalancheTerrain {

    fn get_avalanche_dataset(original_dataset: gdal::Dataset) -> Result<AvalancheTerrain, Box<dyn std::error::Error>> {
        let slope_angles: Vec<f32> = slope_angle_helpers::compute_slope_angle_from_dataset(&original_dataset)?;
        let slope_dataset = slope_angle_helpers::convert_slope_vector_to_dataset(&original_dataset, slope_angles)?;
        let contours = slope_angle_helpers::compute_contours_from_slope_angles(&slope_dataset)?;            
        // Todo: get contours onto dataset as a layer

        let geometry = geojson::Geometry::from(&contours);
        let feature = geojson::Feature {
            geometry: Some(geometry),
            ..geojson::Feature::default()
        };

        Ok(AvalancheTerrain {avalanche_dataset: slope_dataset, geo_json_out: feature})
    }

    pub fn from_lat_lon(api_key: &str, (lat_north, lon_east, lat_south, lon_west) : (f64, f64, f64, f64)) -> Result<AvalancheTerrain, Box<dyn std::error::Error>> {
        let request = slope_angle_helpers::build_open_topo_get_request(lon_west, lon_east, lat_north, lat_south, api_key);
        let open_topo_req = reqwest::blocking::get(request);
        let Ok(response) = open_topo_req else {
            println!("Got an error return!");
            return Err("Uhhh".into());
        };

        println!("Response code: {}", response.status());

        let geo_tiff = response.bytes()?;

        // File gets deleted when it goes out of scope, which is ok because we copy to a new
        // dataset when making the avalanche dataset
        let mut tmpfile = tempfile::NamedTempFile::new()?;
        let _ = tmpfile.write_all(&geo_tiff)?;
        let _ = tmpfile.flush()?;


        let Ok(gdal_dataset) = gdal::Dataset::open(tmpfile.path()) else {
            println!("Failed to process dataset!");
            return Err("Couldn't process dataset".into());
        };

        let result = Self::get_avalanche_dataset(gdal_dataset);
        result
    }

    pub fn from_file(file_path: &str) -> Result<AvalancheTerrain, Box<dyn std::error::Error>> {
        let Ok(gdal_dataset) = gdal::Dataset::open(std::path::Path::new(file_path)) else {
            println!("Failed to process dataset!");
            return Err("Couldn't process dataset".into()); 
        };

        let result = match Self::get_avalanche_dataset(gdal_dataset) {
            Ok(e) => e,
            Err(v) =>  {
                return Err("Couldn't get avalanche dataset".into());
            }
        };
        Ok(result)
    }

    pub fn get_slope_angle_from_point(&self, (lat, lon) : (f64, f64)) -> Result<f32, Box<dyn std::error::Error>>
    {
        let raster_band = self.avalanche_dataset.rasterband(1)?;
        let (x_size, y_size) = raster_band.size();
        let geo_transform = self.avalanche_dataset.geo_transform()?;
        let (x_index, y_index) = (((lon - geo_transform[0])/geo_transform[1]) as isize, ((lat - geo_transform[3])/geo_transform[5]) as isize);
        let mut buff = vec![0.0; 1];
        let _ = raster_band.read_into_slice((x_index, y_index), (1, 1), (1, 1), &mut buff, None)?;
        Ok(buff[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_test() {
        // I know this is naughty and we shouldn't be doing actual API requests for this test but I
        // am lazy
        let _ = dotenvy::dotenv();
        let open_topo_api_key =  dotenvy::var("OPEN_TOPO_KEY").unwrap();

        let st_helens_terrain = crate::slope_angles::AvalancheTerrain::from_lat_lon(open_topo_api_key.as_str(), (46.2168, -122.1595, 46.17836, -122.2144)).unwrap();
    }
}
    
    
