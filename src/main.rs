mod slope_angles;

fn main() {
    let _ = dotenvy::dotenv();
    let Ok(open_topo_api_key) =  dotenvy::var("OPEN_TOPO_KEY") else {
        eprintln!("Please add an open topo API key to your .env!");
        return;
    };
   
    let Ok(st_helens_terrain) = crate::slope_angles::AvalancheTerrain::from_lat_lon(open_topo_api_key.as_str(), (46.2168, -122.1595, 46.17836, -122.2144)) else {
        println!("unable to get avy terrain");
        return;
    };

    // Serialize to string
    let geojson_string = st_helens_terrain.geo_json_out.to_string();
    
    // Write to file
    let _ = std::fs::write("output.geojson", geojson_string) else {
        println!("Failed to write to file!");
        return;
    };
}
