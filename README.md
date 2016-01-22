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

## Compiling on windows

These instructions are for 64 bit Windows 7, augment if necessary.

You'll need both `mingw-w64` from [here](http://sourceforge.net/projects/mingw-w64/files/latest/download) and `Win64OpenSSL` from [here](http://slproweb.com/products/Win32OpenSSL.html)

### MinGW-64

MinGW 64 is a gcc toolchain which a few of the `hyper` dependencies require:

* Download the [installer](http://sourceforge.net/projects/mingw-w64/files/latest/download) and run it
* In the prompt change the Architecture to `x86_64`
* Write down the install location somewhere to change that PATH variable


### OpenSSL

OpenSSL is needed for hyper also

* Download the OpenSSL [installer](http://slproweb.com/products/Win32OpenSSL.html), selecting the `Win64` variety
* Install this to the default location
* Write down the install location (should be `C:\OpenSSL-Win64` )

### Setting environment variables

We need to ensure that `gcc` is on the command line and `openssl` is in the right place

* Navigate to control panel
* Select `System`
* On the sidebar select `Advanced system settings`
* Select `Environment Variables`
* In your user variables, add the `MinGW-64` bin directory to the `PATH` variable, i.e
  * Variable Name: `PATH`
  * Variable Value: `C:\Program Files\mingw-w64\x86_64-5.3.0-posix-seh-rt_v4-rev0\mingw64\bin`
* Add OpenSSL environment two variables like so:
  * Variable Name: `OPENSSL_LIB_DIR`
  * Variable Value: `C:/OpenSSL-Win64`
  * Variable Name: `OPENSSL_INCLUDE_DIR`
  * Variable Value: `C:/OpenSSL-Win64/include`

### Run cargo build

With everything set correctly navigate to where you've downloaded `rustcloud` and run `cargo build --release`.  

If you ran the build before setting the right environment variables, run `cargo clean`

## Compiling on mac

OpenSSL requires the correct location and can be installed with brew:

```
brew install openssl
```


Once you have that, you can simply run this:
```
export DEP_OPENSSL_INCLUDE=$(brew --prefix openssl)/include
cargo build
```

