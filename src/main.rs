mod slope_angles;

use clap::Parser;

/// Program for computing avalanche terrain of a given area. Can run as web server or command line
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Weather or not to run as web server. If not running as webserver, provide output file and
    /// coords
    #[arg(short, long)]
    use_webserver: bool,
    /// IP Address to bind to when running as webserver
    #[arg(short, long, required_if_eq("use_webserver", "true"))]
    ip_address: Option<String>,
    /// Port to bind to when running as webserver 
    #[arg(short, long, required_if_eq("use_webserver", "true"))]
    port: Option<String>,
    /// Northern bounding box lattitude (decimal)
    #[arg(short, long, required_unless_present = "use_webserver")]
    north_lat: Option<f64>,
    /// Southern bounding box lattitude (decimal)
    #[arg(short, long, required_unless_present = "use_webserver")]
    south_lat: Option<f64>,
    /// Eastern bounding box longitude (decimal)
    #[arg(short, long, required_unless_present = "use_webserver")]
    east_lon: Option<f64>,
    /// Western bounding box longitude (decimal)
    #[arg(short, long, required_unless_present = "use_webserver")]
    west_lon: Option<f64>,
    /// Output filename for GeoJson
    #[arg(short, long, required_unless_present = "use_webserver")]
    output_path: Option<String>
}

fn get_from_args(args: &Args, open_topo_api_key: &String) {
    let (north_lat, south_lat, east_lon, west_lon) = match (args.north_lat, args.south_lat, args.east_lon, args.west_lon) {
        (Some(n), Some(s), Some(e), Some(w)) => (n,s, e, w),
        _ => unreachable!("Whoops this shouldn't happen"),
    };

    let Ok(requested_terrain) =  crate::slope_angles::AvalancheTerrain::from_lat_lon(open_topo_api_key.as_str(), (north_lat, east_lon, south_lat, west_lon)) else {
        println!("unable to get avy terrain");
        return;
    };

    // Serialize to string
    let geojson_string = requested_terrain.geo_json_out.to_string();
    
    let output_path_str: &String = &args.output_path.as_ref().unwrap();

    // Write to file
    let _ = std::fs::write(output_path_str, geojson_string) else {
        println!("Failed to write to file!");
        return;
    };
}

fn start_webserver(ip: &String, port: &String, api_key: &String) {
    println!("Starting web server on {ip}:{port}");
}

fn main() {
    let _ = dotenvy::dotenv();
    let Ok(open_topo_api_key) =  dotenvy::var("OPEN_TOPO_KEY") else {
        eprintln!("Please add an open topo API key to your .env!");
        return;
    };

    let args = Args::parse();
    if args.use_webserver {
        let (ip, port) = match(args.ip_address, args.port) {
            (Some(i), Some(p)) => (i, p),
            _ => unreachable!("Whoops"),
        };
        start_webserver(&ip, &port, &open_topo_api_key);
    }
    else {
        get_from_args(&args, &open_topo_api_key);
    }

}
