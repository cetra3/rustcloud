# Rustcloud

Download mixes from soundcloud for offline playback.

A mix is a track that is more than a certain amount of time, which you can configure as a minimum song duration.  It will automatically download via the download link if available, or the stream link if not.  A good minimum duration is 15 or 30 minutes.

It will get the last 300 items from your stream and store them in the following format:

```
<YYYY>/<MM>/<Artist> - <Title>.mp3
```

## Usage

* Sign up for a soundcloud application at [http://soundcloud.com/you/apps](http://soundcloud.com/you/apps) and record your `client_id` and `client_secret`

* Run the `rustcloud` command via terminal in the directory you want to store mixes. You'll be prompted to create a settings file the first time:


```
Please enter your client_id:
<client_id>
Please enter your client_secret:
<client_secret>
Please enter your username (email):
<email>
Please enter your password:
<password>
Please enter your minimum song duration (in minutes):
<duration_minutes>
```

* The settings file is in json format and is stored at `auth_info` in the directory you run rustcloud

* Wait for the mixes to download


## Compiling

You will need [rust](https://www.rust-lang.org).  On Linux you will need openssl dev headers.

With Rust and Cargo installed, run `cargo build` or `cargo build --release`.
