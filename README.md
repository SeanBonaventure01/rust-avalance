## Overview
This this is primarily a rust learning project for myself. The goal is to map avalanche risk areas based of topo data. The basic idea is a rust backend fetches topo data from some provider (like Open Topography), and processes that to get the lat/long of avalanche risk areas to display to users.

The goal is only to not use any AI generated code for the rust of things, and to only use AI tools for light assistance (like asking about various Rust topics or learning more about available APIs)

Everything in the web/ folder was AI generated. I am not trying to get a frontend working. Thats too much more a mere firmware engineer...
Caddy is used to serve the static webpage, do the TLS stuff, then forwards to the rust server. 

## How to run
There are two run modes: command line and web server.

### Command line
This mode is simple, just run:
`cargo run -- --north-lat YOUR_LAT --south-lat YOUR_LAT --east-long YOUR_LONG --west-lon YOUR_LONG -o my_geojson.json`
This will automatically fetch the data from the internet, calculate the slope angles, and identify hazard areas.

### Webserver
First, Caddy is needed to server the static home file:
caddy start

Then, run the binary with the `-u` flag to enable the webserver, using `-i` and `-p` for ip and port
`cargo run -- -u -i 127.0.0.1 -p 8080`


