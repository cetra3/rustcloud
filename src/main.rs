use std::path::Path;
use std::sync::LazyLock;
use std::thread::available_parallelism;

use audiotags::TagType;
use regex::Regex;
use std::collections::HashSet;

use audiotags::{MimeType, Picture, Tag};
use futures::stream::{self, StreamExt, TryStreamExt};
use tokio::fs::{self};

use anyhow::{Error, Result, anyhow};

use serde::{Deserialize, Serialize};

use term_painter::Color::*;
use term_painter::ToStyle;

use gst::prelude::*;
use gstreamer as gst;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserDetails {
    id: i64,
    username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Track {
    id: i64,
    duration: usize,
    title: String,
    permalink_url: String,
    downloadable: Option<bool>,
    user: UserDetails,
    artwork_url: Option<String>,
    download_url: Option<String>,
    stream_url: Option<String>,
    created_at: String,
    kind: String,
    media: Media,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranscodingFormat {
    protocol: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transcoding {
    url: String,
    preset: String,
    format: TranscodingFormat,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Media {
    transcodings: Vec<Transcoding>,
}

#[derive(Serialize, Deserialize)]
pub struct CollectionInfo {
    track: Option<Track>,
}

#[derive(Serialize, Deserialize)]
pub struct Activities {
    collection: Vec<CollectionInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct Settings {
    client_id: String,
    oauth_token: String,
    duration_minutes: usize,
    // Change this if you always wanna download a few tracks
    #[serde(default)]
    min_tracks: usize,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    gst::init()?;

    let auth_info = get_or_prompt_auth_info();

    let duration_limit_ms = auth_info.duration_minutes * 60000;

    // For going back in time! gotta do this as a cli arg
    let mut tracks = Vec::new();

    let limit = 100;
    let mut offset = 0;

    //Filter out duplicates
    let mut processed_tracks: HashSet<i64> = HashSet::new();

    loop {
        let tracks_list = get_tracks(
            &auth_info.oauth_token,
            &auth_info.client_id,
            duration_limit_ms,
            limit,
            offset,
            &mut processed_tracks,
        )
        .await?;

        tracks.extend(tracks_list);

        offset += limit - 1;

        if tracks.len() >= auth_info.min_tracks {
            break;
        }
    }

    let num_tracks = tracks.len();

    println!("Checking {} tracks", num_tracks);

    for track in tracks.iter() {
        let file_path = get_track_path(track, false);
        create_parent_dirs(&file_path).await?;
    }

    let stream = stream::iter(tracks).map(|val| Ok(val) as Result<_, Error>);

    let oauth_token = &*auth_info.oauth_token;

    stream
        .try_for_each_concurrent(available_parallelism()?.get(), |track| async move {
            let file_path = get_track_path(&track, false);

            if fs::metadata(&file_path).await.is_ok() {
                print!("Already Downloaded: ");
                display_track(&track);
                println!(" at: {}", file_path);
            } else {
                print!("Downloading: ");
                display_track(&track);
                println!(" to: {}", file_path);

                if let Err(err) = download_track(oauth_token, &track).await {
                    print!("{}", BrightRed.bold().paint("Error Downloading: "));
                    display_track(&track);
                    println!("Debug : {track:#?}");
                    println!("Error: {err:?}");
                } else {
                    print!("Finished Downloading: ");
                    display_track(&track);
                    println!();
                }
            }

            Ok(())
        })
        .await?;

    Ok(())
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
        oauth_token: prompt_for("oauth_token"),
        duration_minutes: prompt_for("minimum track duration (in minutes)")
            .parse()
            .expect("duration is not a number"),
        min_tracks: 0,
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

#[derive(Serialize, Deserialize)]
struct HlsResponse {
    url: String,
}

const DECODE_PRIORITY: [&str; 4] = ["aac_256k", "aac_160k", "aac_", "mp3_"];

fn sort_by_priority(items: &mut [Transcoding]) {
    items.sort_unstable_by(|a, b| {
        let pos_a = DECODE_PRIORITY
            .iter()
            .position(|p| a.preset.starts_with(*p))
            .unwrap_or(usize::MAX);

        let pos_b = DECODE_PRIORITY
            .iter()
            .position(|p| b.preset.starts_with(*p))
            .unwrap_or(usize::MAX);

        pos_a.cmp(&pos_b)
    });
}

async fn download_track(oauth_token: &str, track: &Track) -> Result<()> {
    let client = reqwest::Client::new();

    let mut transcodings = track.media.transcodings.clone();
    sort_by_priority(&mut transcodings);

    let hls_stream = transcodings
        .first()
        .ok_or_else(|| anyhow!("Found no media transcodings"))?;

    let res = client
        .get(&hls_stream.url)
        .header("Authorization", format!("OAuth {oauth_token}"))
        .send()
        .await?;

    let hls_response: HlsResponse = res.json().await?;

    let file_name = get_track_path(track, false);

    let temp_filename = get_track_path(track, true);
    let temp_dl = temp_filename.clone();

    tokio::task::spawn_blocking(move || download_to_file(&hls_response.url, &temp_dl)).await??;

    let tag_type = if hls_stream.preset.starts_with("aac") {
        TagType::Mp4
    } else {
        TagType::Id3v2
    };

    let tag = Tag::new().with_tag_type(tag_type);

    let mut tag = tag
        .read_from_path(&temp_filename)
        .or_else(|_| tag.create_new())?;
    tag.set_title(&track.title);
    tag.set_artist(&track.user.username);

    if let Some(ref url) = track.artwork_url {
        let larger_url = url.replace("-large.", "-t500x500.");

        let res = client.get(&larger_url).send().await?;
        let buf = res.bytes().await?;

        let cover = Picture {
            mime_type: if larger_url.ends_with("png") {
                MimeType::Png
            } else {
                MimeType::Jpeg
            },
            data: &*buf,
        };

        tag.set_album_cover(cover);
    };

    let album = format!("{} - {}", track.user.username, track.title);

    tag.set_album_title(&album);

    let temp_tag = temp_filename.clone();

    tokio::task::spawn_blocking(move || tag.write_to_path(&temp_tag)).await??;

    tokio::fs::rename(temp_filename, file_name).await?;

    Ok(())
}

fn download_to_file(url: &str, file_name: &str) -> Result<()> {
    let pipeline_str = format!(
        "
        souphttpsrc location=\"{url}\" !
        hlsdemux !
        filesink location=\"{file_name}\"
        "
    );

    let pipeline = gst::parse::launch(&pipeline_str)?;

    let pipeline = pipeline.dynamic_cast::<gst::Pipeline>().unwrap();

    // Get the bus
    let bus = pipeline.bus().unwrap();

    pipeline.set_state(gst::State::Playing)?;

    while let Some(msg) = bus.timed_pop(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(..) => {
                break;
            }
            MessageView::Error(err) => {
                pipeline.set_state(gst::State::Null)?;
                return Err(anyhow!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                ));
            }
            _ => {}
        }
    }

    pipeline.set_state(gst::State::Null)?;

    Ok(())
}

fn display_track(track: &Track) {
    print!(
        "[{}] {} [{}]",
        BrightGreen.bold().paint(track.user.username.clone()),
        BrightCyan.paint(track.title.clone()),
        BrightBlue.paint(format_time(track.duration)),
    )
}

static RE_TRIMMER: LazyLock<Regex> = LazyLock::new(|| Regex::new("[\\|/<>:\"?*]").unwrap());

fn get_track_path(track: &Track, temp: bool) -> String {
    let trimmed_title = RE_TRIMMER.replace_all(&track.title, "");
    let trimmed_username = RE_TRIMMER.replace_all(&track.user.username, "");

    let seperator = match cfg!(target_os = "windows") {
        true => "\\",
        false => "/",
    };

    let mut transcodings = track.media.transcodings.clone();
    sort_by_priority(&mut transcodings);

    let suffix = if transcodings
        .first()
        .is_some_and(|val| val.preset.starts_with("aac"))
    {
        "m4a"
    } else {
        "mp3"
    };

    let temp = if temp { "__tmp" } else { "" };

    let year = &track.created_at[0..4];
    let month = &track.created_at[5..7];

    format!(
        "{year}{seperator}{month}{seperator}{trimmed_username} - {trimmed_title}{temp}.{suffix}",
    )
}

async fn create_parent_dirs(file: &str) -> Result<()> {
    let path = Path::new(file);
    fs::create_dir_all(path.parent().ok_or_else(|| anyhow!("No Parent Path"))?).await?;

    Ok(())
}

fn format_time(time_ms: usize) -> String {
    let hours = time_ms / 3600000;
    let minutes = (time_ms / 60000) % 60;
    let seconds = (time_ms / 1000) % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

async fn get_tracks(
    oauth_token: &str,
    client_id: &str,
    duration_limit_ms: usize,
    limit: usize,
    offset: usize,
    processed_tracks: &mut HashSet<i64>,
) -> Result<Vec<Track>> {
    //max limit is 319???
    let activity_url = format!(
        "https://api-v2.soundcloud.com/stream?client_id={client_id}&limit={limit}&offset={offset}"
    );

    println!("Downloading Activity Feed. Offset {offset}");

    // Create a client.
    let client = reqwest::Client::new();

    // Creating an outgoing request.
    let res = client
        .get(&activity_url)
        .header("Authorization", format!("OAuth {oauth_token}"))
        .send()
        .await?;

    let activity_feed: Activities = res.json().await?;

    let mut tracks: Vec<Track> = Vec::new();

    for collection_info in activity_feed.collection {
        if let Some(track) = collection_info.track {
            if track.duration >= duration_limit_ms
                && !processed_tracks.contains(&track.id)
                && track.kind == "track"
            {
                let file_path = get_track_path(&track, false);

                if fs::metadata(&file_path).await.is_ok() {
                    print!("Already Downloaded: ");
                    display_track(&track);
                    println!(" at: {}", file_path);
                } else {
                    processed_tracks.insert(track.id);
                    tracks.push(track);
                }
            } else {
                print!("Skipping: ");
                display_track(&track);
                println!(" ({})", track.kind);
            }
        }
    }

    Ok(tracks)
}
