extern crate hyper;
extern crate rustc_serialize;
extern crate ansi_term;
extern crate id3;
extern crate chrono;
extern crate filetime;

use std::path::Path;
use std::io::{self, Write, Read, Error, ErrorKind, Result};
use std::fs::{self,File};
use std::collections::HashSet;
use std::thread;

use hyper::Client;
use hyper::header::ContentType;
use rustc_serialize::json;
use ansi_term::{Style,Colour};
use id3::Tag;
use chrono::Local;
use filetime::FileTime;

#[derive(RustcDecodable, RustcEncodable)]
pub struct AuthReponse  {
    access_token: String,
    expires_in: i64
}

#[derive(RustcDecodable, RustcEncodable)]
pub struct UserDetails {
    id: i64,
    username: String
}

#[derive(RustcDecodable, RustcEncodable)]
pub struct SoundObject  {
    id: i64,
    duration: i64,
    title: String,
    permalink_url: String,
    downloadable: Option<bool>,
    user: UserDetails,
    created_at: String,
    kind: String
}

#[derive(RustcDecodable, RustcEncodable)]
pub struct CollectionInfo {
    origin: SoundObject
}


#[derive(RustcDecodable, RustcEncodable)]
pub struct Activities  {
    collection: Vec<CollectionInfo>
}

#[derive(RustcDecodable, RustcEncodable)]
pub struct Settings {
    username: String,
    password: String,
    client_id: String,
    client_secret: String,
    duration_minutes: String
}


fn main() {

    let auth_info = get_or_prompt_auth_info();

    let duration_minutes: i64 = auth_info.duration_minutes.parse().unwrap();

    let auth : AuthReponse = match resolve_file("auth_response") {
        Ok(file) => {

            let metadata = file.metadata().unwrap();

            match read_file(file) {
                Ok(contents) => match json::decode(&*contents) {
                    Ok(json_contents) => {

                        let json: AuthReponse = json_contents;

                        let mtime = FileTime::from_last_modification_time(&metadata);
                        let current_time = Local::now();

                        match (json.expires_in + mtime.seconds_relative_to_1970() as i64) < current_time.timestamp() {
                            true => get_and_save_auth_token(auth_info),
                            false => json
                        }
                    },
                    Err(_) => get_and_save_auth_token(auth_info)
                },
                Err(_) => panic!("Could not decode the auth_response file!")
            }
        },
        Err(_) => get_and_save_auth_token(auth_info)
    };


    let duration_limit_ms = duration_minutes * 60000;

    let songs = get_songs(&auth.access_token, duration_limit_ms);

    let handles: Vec<_> = songs.into_iter().map(|song| {

        let access_token = auth.access_token.clone();

        //Create dirs before multi-thread to avoid conflicts
        let file_path = get_song_path(&song);
        create_parent_dirs(&file_path);

        thread::spawn(move || {

            let cli_out = match cfg!(target_os = "unix") {
                true => format!("[{}] {} [{}]", Style::new().bold().paint(song.user.username.clone()), Colour::Blue.paint(song.title.clone()), format_time(song.duration)),
                false => format!("[{}] {} [{}]",song.user.username, song.title, format_time(song.duration))
            };

            match resolve_file(&file_path){
                Ok(_) => {
                    //println!("Already Downloaded: {}", cli_out);
                },
                Err(_) => {

                    if cfg!(target_os = "unix") {
                        println!("Downloading: {} to: {}", cli_out, Colour::Green.paint(file_path));
                    } else {
                        println!("Downloading: {} to: {}", cli_out, file_path);
                    }
                    download_song(&access_token, &song);
                    println!("Finished Downloading: {}", cli_out);
                }
            };
        })
    }).collect();

    for h in handles {
        h.join().unwrap();
    }
}

fn get_or_prompt_auth_info() -> Settings {

    match resolve_file("auth_info") {
        Ok(file) => match read_file(file) {
            Ok(contents) => match json::decode(&*contents) {
                Ok(json_contents) => {
                    json_contents
                },
                Err(_) => prompt_and_save_auth_info()
            },
            Err(_) => prompt_and_save_auth_info()
        },
        Err(_) => prompt_and_save_auth_info()
    }
}

fn prompt_and_save_auth_info() -> Settings {

    let auth_info = Settings {
        client_id : prompt_for("client_id"),
        client_secret : prompt_for("client_secret"),
        username : prompt_for("username (email)"),
        password : prompt_for("password"),
        duration_minutes: prompt_for("minimum song duration (in minutes)")
    };

    let file_contents = json::encode(&auth_info).unwrap();
    let mut f = File::create("auth_info").unwrap();
    f.write_all(file_contents.as_bytes()).unwrap();

    match resolve_file("auth_response") {
        Ok(_) => fs::remove_file("auth_response").unwrap(),
        Err(_) => ()
    }

    auth_info
}

fn prompt_for(field: &str) -> String {
    let stdin = io::stdin();

    println!("Please enter your {}:", field);

    let mut output = String::new();
    let mut input = String::new();
    match stdin.read_line(&mut input) {
        Ok(_) => {
            output = format!("{}", input.trim());
        }
        Err(_) => ()
    }

    output
}

fn download_song(access_token: &str, song: &SoundObject) {

    let download_or_stream = match song.downloadable {
        Some(can_download) => {
                match can_download {
                    true => "download",
                    false => "stream"
                }
            },
        None => "stream"
    };

    let url = format!("https://api.soundcloud.com/tracks/{}/{}?oauth_token={}", song.id, download_or_stream, access_token);

    let file_name = get_song_path(&song);

    download_to_file(&url, &file_name);

    let mut tag = Tag::with_version(4);
    tag.set_title(song.title.clone());
    tag.set_artist(song.user.username.clone());
    tag.write_to_path(file_name).unwrap();

}

fn download_to_file(url: &str, file_name: &str) {

    let mut file_handle = File::create(file_name.clone()).unwrap();

    // Create a client.
    let client = Client::new();

    let mut res = client.get(url)
                        .send()
                        .unwrap();


    let mut buffer = [0; 10240];

    loop {
        match res.read(&mut buffer[..]) {
            Ok(size) => {

                if size == 0 {
                    break;
                }

                file_handle.write_all(&buffer[0..size]).unwrap()
            },
            Err(_) => break
        }
    }

}

#[cfg(unix)]
fn get_song_path(song: &SoundObject) -> String {
    format!("{}/{} - {}.mp3", &song.created_at[0..7], song.user.username, song.title.replace("/", ""))
}

#[cfg(windows)]
fn get_song_path(song: &SoundObject) -> String {
    let chars_to_trim = &["\\", "|", "/", "<", ">", ":", "\"", "?", "*"];
    let mut trimmed_title = song.title.clone();
    let mut trimmed_username = song.user.username.clone();

    for bad_char in chars_to_trim {
        trimmed_title = trimmed_title.replace(bad_char, "");
        trimmed_username = trimmed_username.replace(bad_char, "");
    }

    format!("{}\\{} - {}.mp3", &song.created_at[0..7].replace("/", "\\"), trimmed_username, trimmed_title)
}

fn create_parent_dirs(file: &str) {
    let path = Path::new(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
}

fn format_time(time_ms: i64) -> String {

    let hours = time_ms / 3600000;
    let minutes = (time_ms / 60000) % 60;
    let seconds = (time_ms / 1000) % 60;

    return format!("{:02}:{:02}:{:02}",hours, minutes, seconds);
}

fn read_file(mut file: File) -> Result<String> {
    let mut s = String::new();
    match file.read_to_string(&mut s) {
        Ok(_) => Ok(s),
        Err(_) => Err(Error::new(ErrorKind::InvalidInput,
                      "the file cannot be read"))
    }
}

fn resolve_file(search_path: &str) -> Result<File> {
    let path = Path::new(search_path);
    match path.exists(){
        true => File::open(&path),
        false => { Err(Error::new(ErrorKind::NotFound,
            "the file cannot be found")) }
        }
}

fn get_songs(access_token: &str, duration_limit_ms: i64) -> Vec<SoundObject> {

    //max limit is 319???
    let activity_url = format!("https://api.soundcloud.com/me/activities?limit=319&oauth_token={}", access_token);

    //Filter out duplicates
    let mut processed_songs: HashSet<i64> = HashSet::new();

    // Create a client.
    let client = Client::new();

    // Creating an outgoing request.
    let mut res = client.get(&activity_url)
                        .send()
                        .unwrap();

    let mut body = String::new();
    res.read_to_string(&mut body).unwrap();


    let activity_feed: Activities = json::decode(&*body).unwrap();

    let mut songs: Vec<SoundObject> = Vec::new();

    for collection_info in activity_feed.collection {
        if collection_info.origin.duration >= duration_limit_ms && !processed_songs.contains(&collection_info.origin.id) && collection_info.origin.kind == "track" {

            processed_songs.insert(collection_info.origin.id);
            songs.push(collection_info.origin);
        } else {
            //println!("Skipping: [{}] {} [{}] ({})", Style::new().bold().paint(collection_info.origin.user.username), Colour::Blue.paint(collection_info.origin.title), format_time(collection_info.origin.duration), collection_info.origin.kind);
        }
    }

    return songs;
}

fn get_and_save_auth_token(auth: Settings) -> AuthReponse {

    // Create a client.
    let client = Client::new();

    let url = "https://api.soundcloud.com/oauth2/token";

    let request =  &*format!("client_id={}&client_secret={}&grant_type=password&username={}&password={}", auth.client_id, auth.client_secret, auth.username, auth.password);

    // Creating an outgoing request.
    let mut res = client.post(url)
                        .header(ContentType::form_url_encoded())
                        .body(request)
                        .send()
                        .unwrap();

    // Read the Response.
    let mut body = String::new();
    res.read_to_string(&mut body).unwrap();

    let mut f = File::create("auth_response").unwrap();


    f.write_all(body.as_bytes()).unwrap();

    println!("Status:{}", res.status);

    return json::decode(&*body).unwrap();

}
