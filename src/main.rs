use std::path::Path;

use chrono::DateTime;
use id3::frame::Picture;
use id3::frame::PictureType;
use regex::Regex;
use std::collections::HashSet;

use futures::stream::{self, StreamExt, TryStreamExt};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use tokio::io::BufWriter;

use anyhow::{anyhow, Error, Result};

use chrono::Local;
use id3::Tag;

use serde::{Deserialize, Serialize};

use term_painter::Color::*;
use term_painter::ToStyle;

#[derive(Deserialize, Serialize, Clone)]
pub struct AuthReponse {
    access_token: String,
    expires_in: i64,
}
#[derive(Deserialize, Serialize)]
pub struct SavedAuthReponse {
    res: AuthReponse,
    date: DateTime<Local>,
}

#[derive(Serialize, Deserialize)]
pub struct UserDetails {
    id: i64,
    username: String,
}

#[derive(Serialize, Deserialize)]
pub struct SoundObject {
    id: i64,
    duration: i64,
    title: String,
    permalink_url: String,
    downloadable: Option<bool>,
    user: UserDetails,
    artwork_url: Option<String>,
    download_url: Option<String>,
    stream_url: Option<String>,
    created_at: String,
    kind: String,
}

#[derive(Serialize, Deserialize)]
pub struct CollectionInfo {
    origin: Option<SoundObject>,
}

#[derive(Serialize, Deserialize)]
pub struct Activities {
    collection: Vec<CollectionInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct Settings {
    username: String,
    password: String,
    client_id: String,
    client_secret: String,
    duration_minutes: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let auth_info = get_or_prompt_auth_info();

    let duration_minutes: i64 = auth_info.duration_minutes.parse().unwrap();

    let auth = match get_auth_response().await {
        Ok(val) => val,
        Err(err) => {
            println!("{}", err);
            get_and_save_auth_token(auth_info).await?
        }
    };

    let duration_limit_ms = duration_minutes * 60000;

    let songs = get_songs(&auth.access_token, duration_limit_ms).await?;

    let num_songs = songs.len();

    println!("Checking {} songs", num_songs);

    for song in songs.iter() {
        let file_path = get_song_path(&song);
        create_parent_dirs(&file_path).await?;
    }

    let stream = stream::iter(songs).map(|val| Ok(val) as Result<_, Error>);

    let access_token = &*auth.access_token;

    stream
        .try_for_each_concurrent(4, |song| async move {
            let file_path = get_song_path(&song);

            if fs::metadata(&file_path).await.is_ok() {
                print!("Already Downloaded: ");
                display_song(&song);
                println!(" at: {}", file_path);
            } else {
                print!("Downloading: ");
                display_song(&song);
                println!(" to: {}", file_path);

                download_song(access_token, &song).await?;
                print!("Finished Downloading: ");
                display_song(&song);
                println!();
            }

            Ok(())
        })
        .await?;

    /*

    for song in songs.into_iter() {
        let access_token = auth.access_token.clone();

        //Create dirs before multi-thread to avoid conflicts
        let file_path = get_song_path(&song);
        create_parent_dirs(&file_path);

        let tx = tx.clone();

        pool.execute(move || {
            match resolve_file(&file_path) {
                Ok(_) => {
                    print!("Already Downloaded: ");
                    display_song(&song);
                    print!(" at: {}\n", file_path);
                }
                Err(_) => {
                    print!("Downloading: ");
                    display_song(&song);
                    print!(" to: {}\n", file_path);

                    download_song(&access_token, &song);
                    print!("Finished Downloading: ");
                    display_song(&song);
                    print!("\n");
                }
            };

            tx.send(()).unwrap();
        });
    }
    */

    Ok(())
}

async fn get_auth_response() -> Result<AuthReponse> {
    let contents = fs::read_to_string("auth_response").await?;

    let val: SavedAuthReponse = serde_json::from_str(&contents)?;

    if (val.date.timestamp() + val.res.expires_in) < Local::now().timestamp() {
        return Err(anyhow!("Expired"));
    }

    return Ok(val.res);
}

fn get_or_prompt_auth_info() -> Settings {
    match std::fs::File::open("auth_info")
        .map_err(Error::from)
        .and_then(|val| serde_json::from_reader(val).map_err(Error::from))
    {
        Ok(settings) => settings,
        Err(_err) => prompt_and_save_auth_info(),
    }
}

fn prompt_and_save_auth_info() -> Settings {
    let auth_info = Settings {
        client_id: prompt_for("client_id"),
        client_secret: prompt_for("client_secret"),
        username: prompt_for("username (email)"),
        password: prompt_for("password"),
        duration_minutes: prompt_for("minimum song duration (in minutes)"),
    };

    let f = std::fs::File::create("auth_info").unwrap();

    serde_json::to_writer(f, &auth_info).unwrap();

    auth_info
}

fn prompt_for(field: &str) -> String {
    let stdin = std::io::stdin();

    println!("Please enter your {}:", field);

    let mut output = String::new();
    let mut input = String::new();
    if stdin.read_line(&mut input).is_ok() {
        output = input.trim().to_string();
    }

    output
}

async fn download_song(access_token: &str, song: &SoundObject) -> Result<()> {
    let download_or_stream = match song.downloadable {
        Some(can_download) => match can_download {
            true => "download",
            false => "stream",
        },
        None => "stream",
    };

    let url = format!(
        "https://api.soundcloud.com/tracks/{}/{}?oauth_token={}",
        song.id, download_or_stream, access_token
    );

    let file_name = get_song_path(&song);

    download_to_file(&url, &file_name).await?;

    let mut tag = Tag::new();
    tag.set_title(song.title.clone());
    tag.set_artist(song.user.username.clone());

    if let Some(ref url) = song.artwork_url {
        let larger_url = url.replace("large.jpg", "t500x500.jpg");

        // Create a client.
        let client = reqwest::Client::new();
        let res = client.get(larger_url).send().await?;
        let buf = res.bytes().await?;

        tag.add_picture(Picture {
            mime_type: "image/jpeg".into(),
            picture_type: PictureType::CoverFront,
            description: "".into(),
            data: buf.to_vec(),
        });
    };

    let album = format!("{} - {}", song.user.username, song.title);

    tag.set_album(album);

    tokio::task::spawn_blocking(move || tag.write_to_path(file_name, id3::Version::Id3v24))
        .await??;

    Ok(())
}

async fn download_to_file(url: &str, file_name: &str) -> Result<()> {
    let out_file = File::create(file_name).await?;

    let mut file_handle = BufWriter::new(out_file);

    // Create a client.
    let client = reqwest::Client::new();

    let mut res = client.get(url).send().await?;

    while let Some(chunk) = res.chunk().await? {
        file_handle.write_all(&chunk).await?;
    }

    file_handle.flush().await?;

    Ok(())
}

fn display_song(song: &SoundObject) {
    print!(
        "[{}] {} [{}]",
        BrightGreen.bold().paint(song.user.username.clone()),
        BrightCyan.paint(song.title.clone()),
        BrightBlue.paint(format_time(song.duration)),
    )
}

fn get_song_path(song: &SoundObject) -> String {
    let re_trimmer = Regex::new("[\\|/<>:\"?*]").unwrap();

    let trimmed_title = re_trimmer.replace_all(&*song.title, "");
    let trimmed_username = re_trimmer.replace_all(&*song.user.username, "");

    let seperator = match cfg!(target_os = "windows") {
        true => "\\",
        false => "/",
    };

    let year = &song.created_at[0..4];
    let month = &song.created_at[5..7];

    format!(
        "{}{}{}{}{} - {}.mp3",
        year, seperator, month, seperator, trimmed_username, trimmed_title
    )
}

async fn create_parent_dirs(file: &str) -> Result<()> {
    let path = Path::new(file);
    fs::create_dir_all(path.parent().ok_or_else(|| anyhow!("No Parent Path"))?).await?;

    Ok(())
}

fn format_time(time_ms: i64) -> String {
    let hours = time_ms / 3600000;
    let minutes = (time_ms / 60000) % 60;
    let seconds = (time_ms / 1000) % 60;

    return format!("{:02}:{:02}:{:02}", hours, minutes, seconds);
}

async fn get_songs(access_token: &str, duration_limit_ms: i64) -> Result<Vec<SoundObject>> {
    //max limit is 319???
    let activity_url = format!(
        "https://api.soundcloud.com/me/activities?limit=200&oauth_token={}",
        access_token
    );

    //Filter out duplicates
    let mut processed_songs: HashSet<i64> = HashSet::new();

    println!("Downloading Activity Feed");

    // Create a client.
    let client = reqwest::Client::new();

    // Creating an outgoing request.
    let res = client.get(&activity_url).send().await?;

    let activity_feed: Activities = res.json().await?;

    let mut songs: Vec<SoundObject> = Vec::new();

    for collection_info in activity_feed.collection {
        if let Some(song) = collection_info.origin {
            if song.duration >= duration_limit_ms
                && !processed_songs.contains(&song.id)
                && song.kind == "track"
            {
                processed_songs.insert(song.id);
                songs.push(song);
            } else {
                print!("Skipping: ");
                display_song(&song);
                println!(" ({})", song.kind);
            }
        }
    }

    return Ok(songs);
}

async fn get_and_save_auth_token(auth: Settings) -> Result<AuthReponse> {
    let client = reqwest::Client::new();
    // Create a client.

    let url = "https://api.soundcloud.com/oauth2/token";

    let request = format!(
        "client_id={}&client_secret={}&grant_type=password&username={}&password={}",
        auth.client_id, auth.client_secret, auth.username, auth.password
    );

    // Creating an outgoing request.
    let res = client
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(request)
        .send()
        .await?;

    println!("Status:{}", res.status());

    let json: AuthReponse = res.json().await?;

    let mut f = File::create("auth_response").await?;

    let saved_auth_response = SavedAuthReponse {
        res: json.clone(),
        date: Local::now(),
    };

    f.write_all(&serde_json::to_vec(&saved_auth_response)?)
        .await?;

    return Ok(json);
}
