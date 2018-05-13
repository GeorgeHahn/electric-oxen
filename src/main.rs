#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate flate2;

use serde_json::Error;
use std::io::prelude::*;
use flate2::read::GzDecoder;
use clap::{Arg, App};
use reqwest::{Client};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    server: String,
    apikey: String,
    debug: bool,
    no_download: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct Season {
    id: u32,
    name: String,
    width: u32,
    height: u32,
    quality: u32,
    nframes: u32,
    gid: u32,
    started: serde_json::Value,
    ended: serde_json::Value,
    active: bool,
    branch: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Work {
    sequence: String,
    frame: u32,
    genome_id: u32,
    genome2_id: u32,
}

lazy_static! {
    static ref CONFIG: Config = {
        let matches = App::new("electric-oxen")
            .version(crate_version!())
            .arg(Arg::with_name("server")
                .long("server")
                .value_name("SERVER")
                .help("Connect to an alternative server")
                .takes_value(true))
            .arg(Arg::with_name("apikey")
                .long("apikey")
                .value_name("APIKEY")
                .help("Your api key")
                .takes_value(true)
                .required(true))
            .arg(Arg::with_name("debug")
                .long("debug")
                .help("Enable debug mode"))
            .arg(Arg::with_name("no-download")
                .long("no-download")
                .help("Disable movie downloading"))
                .get_matches();

        let c = Config { 
            server: String::from(matches.value_of("server").unwrap_or("https://sheeps.triple6.org")),
            apikey: String::from(matches.value_of("apikey").unwrap()),
            debug: value_t!(matches, "debug", bool).unwrap_or(false),
            no_download: value_t!(matches, "no-download", bool).unwrap_or(false)
        };
        c
    };
}

lazy_static! {
    static ref CLIENT: Client = {
        build_client(true).expect("Could not build client")
    };
}

fn build_client(gzip: bool) -> Result<Client, Box<std::error::Error>> {
    // read a local binary DER encoded certificate
    let mut buf = Vec::new();
    File::open("sheeps.der")?.read_to_end(&mut buf)?;
    // create a certificate
    let cert = reqwest::Certificate::from_der(&buf)?;
    // get a client builder
    let c = reqwest::Client::builder()
        .gzip(gzip)
        .add_root_certificate(cert)
        .build()?;
    Ok(c)
}

fn main() {
    // TODO: Create dirs flames/ and frames/

    let mut count = 0u32;
    loop {
        // TODO: Convert to state machine with retries
        println!("Rendered {} frame(s)", count);
        let _season = get_active_season().expect("Unable to get active season");
        let _work = request_work().expect("Unable to get work");

        for _w in _work {
            let _genome_file = get_genome(&_season, &_w).expect("Unable to get genome");
            let _frame = render_frame(_genome_file, &_season, &_w);
            upload_frame(_frame.expect("Bad frame"), &_season, &_w).expect("Failed to upload frame");
            count += 1;
        }
    }
}

/// Request information about the currently active season
/// {"id":79,"name":"G244_W1920_H1080_Q50000_KLo56aTAGPU","width":1920,"height":1080,"quality":50000,"nframes":120,"gid":244,"started":"2018-05-10 04:56:49 +0200","ended":null,"active":true,"branch":"gpu"}
fn get_active_season() -> Result<Season, reqwest::Error> {
    let _url = format!("{}/api/active_season?apikey={}&gpu=true", CONFIG.server, CONFIG.apikey);
    let _response = CLIENT.get(&_url)
        .send().expect("Unable to request active season: request failed")
        .json().expect("Unable to request active season: invalid response");
    Ok(_response)
}

///"[{"sequence":"electricsheep.244.00001.01696_electricsheep.244.00001.01696","frame":0,"genome_id":13903,"genome2_id":13903}]"
fn request_work() -> Result<Vec<Work>, reqwest::Error> {
    let _url = format!("{}/api/request_work?apikey={}&gpu=true", CONFIG.server, CONFIG.apikey);
    let _response = CLIENT.get(&_url)
        .send().expect("Unable to request work: request failed")
        .json().expect("Unable to request work: invalid response");
    Ok(_response)
}

fn get_genome(season: &Season, _work: &Work) -> Result<PathBuf, reqwest::Error> {
    let _path = format!("flames/{}.flame", _work.sequence);
    let _flame = Path::new(_path.as_str());
    if _flame.exists() {
        return Ok(_flame.to_path_buf());
    }

    // Download the flame if it doesn't exist
    let _url = format!("{}/{}/animated_genomes/{}.flame.gz", CONFIG.server, season.name, _work.sequence);
    let mut buf: Vec<u8> = vec![];
    build_client(false).expect("Unable to build download client")
        .get(&_url).send().expect("Unable to download flame: request failed")
        .copy_to(&mut buf).expect("Unable to copy flame");

    let mut d = GzDecoder::new(buf.as_slice());
    let mut s = String::new();
    d.read_to_string(&mut s).unwrap();

    let mut file = File::create(_flame).expect("Unable to create flame file");
    file.write_all(s.as_bytes()).expect("Unable to write flame file");
    Ok(_flame.to_path_buf())
}

fn render_frame(_genome_file: PathBuf, _season: &Season, _w: &Work) -> Result<PathBuf, ()> {
    let outputfile = format!("frames\\{}.{}.{}.jpg", _season.name, _w.sequence, _w.frame);
    let renderer = "C:\\Users\\georg\\AppData\\Roaming\\Fractorium\\emberanimate.exe"; // TODO
    
    let input = format!("--in={}", _genome_file.to_string_lossy());
    let frame = format!("--frame={}", _w.frame - 1);
    let quality = format!("--quality={}", _season.quality);
    let prefix = format!("--prefix=..\\frames\\{}.{}.", _season.name, _w.sequence);
    if cfg!(target_os = "windows") {
        let _output = Command::new(renderer)
                    .arg(input)
                    .arg("--format=jpg")
                    .arg("--opencl")
                    .arg("--priority=-2") // TODO
                    .arg(quality)
                    .arg("--sp")
                    .arg(prefix)
                    .arg("--supersample=2")
                    .arg(frame)
                    .arg("--isaac_seed=fractorium")
                    .output()
                    .expect("failed to execute process");
        println!("Frame {} {:?}", _w.frame, String::from_utf8_lossy(_output.stdout.as_slice()));
    }

    Ok(Path::new(outputfile.as_str()).to_path_buf())
}

fn upload_frame(_frame: PathBuf, _season: &Season, _w: &Work) -> Result<(), reqwest::Error> {
    let _url = format!("{}/api/upload", CONFIG.server);
    let api = format!("{}", CONFIG.apikey);
    let sname = format!("{}", _season.name);

    let form = reqwest::multipart::Form::new()
        .file("file", _frame.to_str().unwrap()).unwrap()
        .text("apikey", api)
        .text("work_set", serde_json::to_string(&_w).unwrap())
        .text("branch", sname)
        .text("gpu", "true");

    let res = CLIENT.post(&_url)
        .multipart(form)
        .send().expect("Failed to upload file")
        .text()?;

    println!("{}", res);
    Ok(())
}
