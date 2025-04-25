# Rustcloud

Download mixes from soundcloud for offline playback.  This will find the best quality you have available for your account, downloading it, converting it to mp3 and storing the artwork/track information.

A mix is a track that is more than a certain amount of time, which you can configure as a minimum song duration. A good minimum duration is 15 or 30 minutes.

It will get the last ~200 items from your stream and store them in the following format:

```
<YYYY>/<MM>/<Artist> - <Title>.mp3
```

## Usage

This used to use the official API but they have removed access to signing up.....

* Login to soundcloud and navigate to your stream
* Open dev tools and search for any url request to `api-v2`:
    * Grab the client id from the url of the request (it's the `client_id` param)
    * Grab the oauth token from your `Authorization` header (the value after `Oauth `)
* Run the `rustcloud` command via terminal in the directory you want to store mixes. You'll be prompted to create a settings file the first time:

```
Please enter your client_id:
<client_id>
Please enter your oauth_token:
<oauth_token>
Please enter your minimum song duration (in minutes):
<duration_minutes>
```

* The settings file is in json format and is stored at `auth_info` in the directory you run rustcloud

* Wait for the mixes to download

## Compiling

You will need [rust](https://www.rust-lang.org).  You will also need to install `gstreamer` so follow these [install instructions](https://docs.rs/gstreamer/latest/gstreamer/#installation). On Linux you will need openssl dev headers.

With Rust and Cargo installed, run `cargo build` or `cargo build --release`.
