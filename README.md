[![CI](https://github.com/Foalyy/niobium/actions/workflows/ci.yml/badge.svg)](https://github.com/Foalyy/niobium/actions/workflows/ci.yml)

# Niobium


<div align="center"><img src="static/img/niobium.svg" width="200"></div>

### <div align="center">Modern, high-performance, web-based photo gallery viewer</div>

### <div align="center">:framed_picture: :sparkles: :rocket:</div>


![Screenshot](https://user-images.githubusercontent.com/2955191/207928165-247f39c1-826a-481d-a7c0-08a3a804d301.jpg)

### <div align="center">[:fire: Demo](https://demo.niobium.silica.io/)</div>

**Niobium** is an open-source, self-hosted photo gallery viewer that features :
- a clean, full-screen, responsive grid with subtle animations to showcase your best images
  - great for including your galleries in other webpages as `iframe`s !
- recursive indexing of photos and progressive loading for a highly efficient "infinite scroll" display
- automatic generation of lightweight previews of each photo and a high-performance Rust backend for blazingly fast loading of each page
- enlarged (full-screen) display of photos with slideshow mode
- an optional navigation panel to explore sub-directories
- custom _collections_, to create independant galleries each with a customizable URL pointing to a curated selections of photos
- fine-grained password protection and control over the indexing of each directory and collection
- easy self-host on a server or NAS

*Interested in a hosted version for you or your clients? Contact me at the address displayed on [my profile](https://github.com/Foalyy).*

- [1/ Installation](#hammer_and_wrench-1-installation)
  - [1.1/ Linux](#computer-11-linux)
    - [1.1.1/ (Option A) Install with Docker](#ship-111-option-a-install-with-docker)
    - [1.1.1/ (Option B) Install using a release](#package-111-option-b-install-using-a-release)
    - [1.1.1/ (Option C) Build from source](#wrench-111-option-c-build-from-source)
    - [1.1.2/ Start as a daemon](#ghost-112-start-as-a-daemon)
    - [1.1.3/ Set up the reverse proxy](#shield-113-set-up-the-reverse-proxy)
    - [1.1.4/ Updating](#arrow_down-114-updating)
  - [1.2/ Raspberry Pi](#strawberry-12-raspberry-pi)
  - [1.3/ NAS (Synology, ...)](#file_cabinet-13-nas-synology-)
  - [1.4/ Windows](#desktop_computer-14-windows)
- [2/ Configuration](#gear-2-configuration)
  - [2.1/ Environment variables](#globe_with_meridians-21-environment-variables)
  - [2.2/ Main config file](#spiral_notepad-22-main-config-file)
  - [2.3/ Subdirectories config files](#open_file_folder-23-subdirectories-config-files)
- [3/ Reloading](#arrows_counterclockwise-3-reloading)
- [4/ Collections](#framed_picture-4-collections)
- [5/ Acknowledgements](#handshake-5-acknowledgements)

## :hammer_and_wrench: 1/ Installation

## :computer: 1.1/ Linux

### :ship: 1.1.1/ *(Option A)* Install with Docker

If you have Docker installed, this is the easiest solution. You will need 2 directories : 

- the directory where you store your photos (read-only access is enough), for instance `/srv/niobium/photos/`, which must be mounted into `/app/photos` inside the container
- a `data` directory (which must be mounted into `/app/data` inside the container) where the config, database, and cache will be stored, for instance `/srv/niobium/data/` (write access is required)

Then simply download and run Niobium using :

```
$ docker run -v /srv/niobium/photos:/app/photos:ro -v /srv/niobium/data:/app/data -p 8000:8000 niobium
```

This will expose Niobium on port 8000.

The Docker image has a startup script which copies the default config file into the `data` directory the first time the container is launched, and overrides the default value of some environment variables to point to this directory (see `docker-entrypoint.sh` for more information). In order to customize the configuration, launch the app a first time, then edit the newly-created `niobium.config` inside the `data` directory, and finally restart the app to reload the config. There is also a `niobium_collections.config.sample` that can be customized and renamed to `niobium_collections.config` in order to create collections (see [Collections](#framed_picture-4-collections) below).

If you prefer to use `docker-compose`, here is a sample script :

```
version: "3"

services:
  niobium:
    image: niobium
    container_name: niobium
    restart: always
    ports:
      - "8000:8000"
    volumes:
      - /srv/niobium/photos:/app/photos:ro
      - /srv/niobium/data:/app/data
    environment:
      - NIOBIUM_LOADING_WORKERS=4 # Example of config override using environment variables
```


### :package: 1.1.1/ *(Option B)* Install using a release

Example of how to install Niobium on Debian 12 in `/var/www/my_photos`, customize the paths as needed. See the latest release on GitHub : https://github.com/Foalyy/niobium/releases

```
# cd /var/www/
# wget https://github.com/Foalyy/niobium/releases/download/v0.4.2/niobium_0.4.2.zip
# unzip niobium_0.4.2.zip
# mv niobium_0.4.2 my_photos
# cd my_photos
```

Copy your photos to the `photos/` directory here. Alternatively, you can leave your photos folder where it currently is and set the `PHOTOS_DIR` setting to point to that path (see below).

```
# cp -r /path/to/my/photos ./photos
```

Copy the sample configuration file and take a look at it, every setting is documented inside :

```
# cp niobium.config.sample niobium.config
# vim niobium.config
```

Settings that you may want to customize now include :
- `ADDRESS` : it is recommended to install Niobium behind a reverse proxy (provided by Apache or Nginx for instance), in which case leave the default value "127.0.0.1", but if you want direct access you will need to set "0.0.0.0" to open the service on every interfaces
- `PORT` : the default port to bind to is **8000** but it can be customized here
- `TITLE` : the name of your photos folder that will displayed to the user in the navigation panel and the browser's tab
- `PHOTOS_DIR` : if you prefer storing your photos outside the app's directory, specify the path here. Make sure the user that the app will run as (for example, www-data) has read access to this path. Write access is **not** necessary. This may be useful for example if your photos are stored in a specific FTP-accessible directory outside of the app's installation directory.
- `PRE_GENERATE_THUMBNAILS` : set to true if you want Niobium to immediately generate the thumbnails of your photos at startup. Note that this may take some time, depending on the number of photos and your CPU. Otherwise, the thumbnails will be lazily generated and cached when requested for the first time by a user.
- `PASSWORD` : if your photos are private, you may want to protect access with a password. Note that subdirectories can be password-protected (or hidden from the navigation panel) on a per-directory basis, see below.

Start Niobium to make sure everything works fine. Note that photos indexing (and thumbnails generation, if enabled) may take some time during the first launch. Everything will be cached so the next launch will be quick.

```
# ./niobium
```

Niobium is now running, but for a more permanent installation, keep reading [section 1.1.2](#ghost-112-start-as-a-daemon).

### :wrench: 1.1.1/ *(Option C)* Build from source

Niobium is built with Rust, you will need a Rust compiler to compile it. Here is the simplest way to install one, following [official instructions](https://www.rust-lang.org/tools/install), which will install Cargo (an all-in-one tool to easily compile and run Rust programs) and an up-to-date Rust compiler :

```
# curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# source "$HOME/.cargo/env"
```

(The default installation options should be fine in most cases but feel free to take a look.)

Get the source code :

```
# cd /var/www/
# git clone https://github.com/Foalyy/niobium.git my_photos
# cd my_photos
# cargo build --release
```

Cargo will automatically fetch dependencies and compile everything. The app can then be started using :

```
# cargo run --release
```

Create a symlink to the binary into the main directory :

```
# ln -s target/release/niobium niobium
```


### :ghost: 1.1.2/ Start as a daemon

A sample `systemd` service file is provided in `utils/niobium.service`. You can customize it as necessary, and then install it :

```
# vim utils/niobium.service  # Check user and paths
# cp utils/niobium.service /etc/systemd/system/
# chown -R www-data:www-data ./  # Assuming the app will run as the user www-data
# systemctl enable --now niobium.service
# systemctl status niobium.service  # Make sure everything looks fine
```

If you didn't start the app directly with `./niobium`, you can track the progress of the photos indexing using :

```
# journalctl -u niobium.service -f
```

If your OS is not `systemd`-based or your religion forbids you from using `systemd`, adapt the daemon config file accordingly.


### :shield: 1.1.3/ Set up the reverse proxy

Niobium can serve your files directly, but it is recommended to set it up behind a reverse proxy that will handle HTTPS.

Example using Apache with a Let's Encrypt HTTPS certificate. We'll assume you want to host your photos on `photos.example.com`.

Get a certificate :
```
# certbot certonly
```

Create the virtualhost config file for Apache :
```
# vim /etc/apache2/sites-available/niobium.conf
```

You should probably base your config on other existing configs there, but as a reference, here is a simple config file that should work for most cases (remember to customize the domain name) :

```
<IfModule mod_ssl.c>
    <VirtualHost *:443>
        ServerName photos.example.com
        ServerAdmin admin@example.com

        ProxyPass "/" "http://localhost:8000/"

        ErrorLog ${APACHE_LOG_DIR}/niobium_error.log
        CustomLog ${APACHE_LOG_DIR}/niobium_access.log combined

        SSLEngine on
        SSLCertificateFile  /etc/letsencrypt/live/photos.example.com/fullchain.pem
        SSLCertificateKeyFile /etc/letsencrypt/live/photos.example.com/privkey.pem
    </VirtualHost>

    <VirtualHost *:80>
        ServerName photos.example.com
        Redirect permanent / https://photos.example.com/

        ErrorLog ${APACHE_LOG_DIR}/niobium_error.log
        CustomLog ${APACHE_LOG_DIR}/niobium_access.log combined
    </VirtualHost>
</IfModule>
```

Enable and start the virtualhost :

```
# a2ensite niobium.conf
# systemctl reload apache2
```


### :arrow_down: 1.1.4/ Updating

Updating Niobium to the latest version is easy : 
- stop the service if it is running
- if you have installed using a realease :
  - download the latest release
  - extract it over your current installation, to replace the existing files
- if you have built from source :
  - update : `git pull`
  - rebuild : `cargo build --release`
- restart the service


## :strawberry: 1.2/ Raspberry Pi

If you would like to self-host your photos on your own Internet connection, a Raspberry Pi is a great and cost-effective solution. If you have a commercial NAS, [look below](#file_cabinet-13-nas-synology-) instead.

To run Niobium on a Raspberry Pi, [install the latest Raspberry Pi OS](https://www.raspberrypi.com/documentation/computers/getting-started.html#install-an-operating-system), then either :

- [install Docker](https://docs.docker.com/engine/install/debian/), pull [foalyy/niobium](https://hub.docker.com/r/foalyy/niobium), then follow the [instructions for Linux above](#ship-111-option-a-install-with-docker)
- or [install using a release](#package-111-option-b-install-using-a-release)

If you want to allow external access, remember to open and redirect the port on your router.


## :file_cabinet: 1.3/ NAS (Synology, ...)

Some modern NAS operating systems, such as DSM from Synology, are able to run apps packaged with Docker. This makes it very handy to self-host your gallery right where your photos are.

Here are the instructions for Synology, but installation for other NAS manufacturers should be very similar :

- go to the Package Center, look for the Docker app, and make sure that it is installed and running
- open Docker in the apps menu
- in the Registry menu, look for `foalyy/niobium` and download it
- in the Image menu, select `foalyy/niobium:latest` and click Launch (or simply double-click on the image)
- in the configuration assistant, here are a few settings to customize :
  - check `Enable auto-restart`
  - if you are using Web Station, you may want to enable the Web Portal
  - in the Port Settings page, map a local port to the port 8000 inside the container (usually, the local port will also be 8000)
  - in the Volume Settings page, add two folders :
    - point to the folder that contains your photos on your NAS, and mount it in the path `/app/photos` (you can check _Read only_ for this folder)
    - create a new folder on your NAS somewhere that will be used to store the apps database and config files (for instance in the `docker` shared folder), and mount it on `/app/data` (do *not* check "Read only" for this folder)
  - check "Run this container after the wizard is finished"
- the running container should appear in the Image menu, you can open it to check the config and read the logs (notably, the progress of the photos indexing)
- finally, point your browser to the IP address of your NAS with the chosen port, the gallery should appear

If you want to allow external access, remember to open and redirect the port on your router.


## :desktop_computer: 1.4/ Windows

Niobium currently doesn't provide compiled packages for Windows. If you would like to host a photo gallery on a Windows machine, either :

- install Docker Desktop and follow similar instructions as for a [NAS](#file_cabinet-13-nas-synology-)
- or install Rust and and adapt the [instructions above](#wrench-111-option-c-build-from-source) to build from source


## :gear: 2/ Configuration

### :globe_with_meridians: 2.1/ Environment variables

Niobium can be configured using environment variables :

- `NIOBIUM_CONFIG_FILE` : path to the main config file (default : `niobium.config`)
- `NIOBIUM_SECRET_FILE` : file that will be generated with the secret key used for private cookies (default : `.secret`)

Some (but not all) settings from the config can be overriden with environment variables, by prefixing the name of the setting with `NIOBIUM_` :
- `NIOBIUM_ADDRESS`
- `NIOBIUM_PORT`
- `NIOBIUM_TITLE`
- `NIOBIUM_DESCRIPTION`
- `NIOBIUM_PHOTOS_DIR`
- `NIOBIUM_CACHE_DIR`
- `NIOBIUM_DATABASE_PATH`
- `NIOBIUM_LOADING_WORKERS`
- `NIOBIUM_PASSWORD` (can also be specified as a file using `NIOBIUM_PASSWORD_FILE`)
- `NIOBIUM_COLLECTIONS_FILE`

For more information about these, see below.


### :spiral_notepad: 2.2/ Main config file

The main config file, `niobium.config` by default, is self-documented, see below.

After changing a setting, the app needs to be restarted, for example using :
```
# systemctl restart niobium.service
```

`niobium.config` :

```
### Main config file for Niobium.
### After changing settings, the app needs to be restarted.
### Some of these settings are overridable : they can be customized in a subdirectory
### by creating a .niobium.config file inside this directory. See the README and the
### settings at the bottom of this file for more information on settings overrides.


## Server settings

# IP address to serve on. Set to "0.0.0.0" to serve on all interfaces.
# Default : 127.0.0.1 (only accessible locally)
ADDRESS = "127.0.0.1"

# Port to serve on.
# Default : 8000
PORT = 8000


## Identity

# Title displayed in the page title and the top of the navigation panel.
# Default : "Niobium"
TITLE = "Niobium"

# Description displayed at the top of the navigation panel. Can contain HTML :
# be careful not to make editable by a user.
# Default : (empty)
DESCRIPTION = ""

# Instagram handle to link to in the dedicated button at the upper right,
# leave empty to remove the button.
# Default : (empty)
INSTAGRAM = ""


## Files and paths

# Path to the photos folder containing the photos to display.
# Write access is not required.
# Default : "photos" (in the app's folder)
PHOTOS_DIR = "photos"

# Path to the cache folder that will be used by the app to store thumbnails.
# Write access is required.
# Default : "cache" (in the app's folder)
CACHE_DIR = "cache"

# Path to the SQLite database file used by the app to store the photos index. It will
# be automatically created during the first launch, but write access to the containing
# folder is required.
# Default : "niobium.sqlite" (in the app's folder)
DATABASE_PATH = "niobium.sqlite"


## Photos indexing

# If enabled, the app will index subdirectories recursively in the photos folder.
# Default : true
INDEX_SUBDIRS = true

# Number of parallel worker tasks that will be spawned when loading new photos into the
# database.
# Default : 16
LOADING_WORKERS = 16

# If enable, the app will try to read EXIF metadata of photos and save them in the
# database.
READ_EXIF = true

# If enabled, thumbnails will be generated immediately when the photos are loaded into
# the database; otherwise they will be generated on demand when requested by a browser
# for the first time.
# Default : false
PRE_GENERATE_THUMBNAILS = false


## Navigation and subdirectories

# Configure a password needed to access this gallery. Leave empty to disable.
# Default : empty (no password needed)
# This setting is overridable : individual subdirectories can require different passwords.
# See also the HIDDEN settings.
PASSWORD = ""

# If enabled, the grid display for a requested path will show every photo available in
# its subdirectories (therefore the root directory will show every photo in the database).
# Otherwise, only the photos actually inside the requested path will be shown, most like
# a classic file browser.
# Default : true
# This setting is overridable.
SHOW_PHOTOS_FROM_SUBDIRS = true

# If enabled, a navigation panel will be displayed when there are subdirectories in the
# photos folder. Otherwise, only direct links will allow users to access subdirectories.
# Default : true
SHOW_NAVIGATION_PANEL = true

# If enabled, the navigation will be open by default when there are subdirectories in
# the requested path.
# Default : true
OPEN_NAVIGATION_PANEL_BY_DEFAULT = true

# If enabled, the available collections will be displayed in the navigation panel.
# Default : true
SHOW_COLLECTIONS_IN_NAVIGATION_PANEL = true


## User interface

# Fields(s) to use to sort the photos being displayed. This can be a single field or a
# comma-separated list of fields for multi-ordering. Available fields : `id`,
# `filename`, `title`, `date_taken`, `sort_order`.
# Default : "filename"
# This setting is overridable.
SORT_ORDER = "filename"

# If enabled, the sort order of the photos will be reversed.
# Default : false
# This setting is overridable.
REVERSE_SORT_ORDER = false

# Height of a single row displayed in grid view, as a percent of the browser's viewport
# height. For example, `20` will show up to 5 rows at a time. The user can change it
# using Zoom+ and Zoom- buttons in the interface.
# Default : 23 (show 4 rows with a hint of more at the bottom)
DEFAULT_ROW_HEIGHT = 23 # vh

# Percentage by which the grid's row height is modified every time the user presses the
# Zoom+ / Zoom- buttons.
# Default : 10
ROW_HEIGHT_STEP = 10 # %

# In order to display a neat grid with photos of arbitrary ratios, the grid needs to
# crop some photos. This setting defines the maximum amount of crop that can be applied
# before giving up and leaving holes in the grid.
# For example, 1 means no crop is allowed, and 2 means that photos can be cropped to as
# much as half of their original height.
# Default : 2
MAX_CROP = 2

# If enabled, show a button allowing the user to view metadata of photos (such as camera
# model and aperture) in Loupe mode.
# Default : true
SHOW_METADATA = true

# If enabled, the metadata will be visible by default (but can still be hidden by the
# user). Requires `SHOW_METADATA` to be enabled.
# Default : true
METADATA_VISIBLE_BY_DEFAULT = true

# If enabled, the Loupe view will show a button allowing the user to download the photo
# in original quality.
# Default : true
SHOW_DOWNLOAD_BUTTON = true

# Prefix used for the name of downloaded photos. The UID of the photo will be appended
# to it.
# Default : "niobium_"
DOWNLOAD_PREFIX = "niobium_"

# Delay (in milliseconds) to wait before switching to the next photo in Slideshow mode.
# Default : 5000 (5s)
SLIDESHOW_DELAY = 5000 # ms


## Thumbnails and quality

# Max size of thumbnails on any side, in pixels.
# Default : 600
THUMBNAIL_MAX_SIZE = 600 # px, on any side

# Quality used to reencode thumbnails images, in percent.
# Default : 75
THUMBNAIL_QUALITY = 75 # %

# Max size of large-size images in Loupe view on any side, in pixels.
# Default : 1920
LARGE_VIEW_MAX_SIZE = 1920 # px, on any side

# Quality used to reencode large-size images in Loupe view, in percent.
# Default : 85
LARGE_VIEW_QUALITY = 85 # %

# Image format used for resized photos in cache : JPEG or WEBP.
# Default : WEBP
RESIZED_IMAGE_FORMAT = "WEBP"

# Path to the config file that defines the list of collections
# Default : "niobium_collections.config"
COLLECTIONS_FILE = "niobium_collections.config"



## Settings only available for subdirectories
# (Do not uncomment these settings here : they will have no effect. They are only
# provided here for documentation purposes. In order to apply these settings for
# individual subdirectories, create a `.niobium.config` file inside these subdirectories
# and specify these settings here. Any setting marked as "overridable" above can also
# be specified there.)

# If disabled, this directory will be ignored and no file inside it will be indexed.
# Default : true
#INDEX = true

# If enabled, this folder will not be shown in the navigation panel, and a direct link
# will be required to access it.
# Default : false
#HIDDEN = false
```


### :open_file_folder: 2.3/ Subdirectories config files

When your photos folder is sorted into subdirectories, some settings can be customized for specific directories by creating a `.niobium.config` config file (note the leading dot in the filename) inside these directories. Available settings are those marked as "overridable" in the main config file.

For example, you can :
- ignore a certain subdirectory completely by setting `INDEX = false`
- hide a subdirectory from the navigation panel by setting `HIDDEN = true`
- specify a password required to access a subdirectory by setting `PASSWORD = "1337P455W0RD"`



## :arrows_counterclockwise: 3/ Reloading

When the app launches, the photos index is cached in memory to improve performances. If you add or remove photos from `PHOTOS_DIR`, or if you change some subdirectories configuration files, the index needs to be synchronized with the photos on disk. A full restart of the app will do the job, but the quickest way is to simply open the special `.reload` URL from any browser. For example, if your photos are accessible on `https://photos.example.com/`, simply open the page at `https://photos.example.com/.reload`. The photos index will be reloaded and synchronized with the internal database, and you will simply be redirected to the root page showing your new photos.

This will *not* reload the main configuration file, but it *will* reload the `.niobium.config` configuration files in your photos folder.


## :framed_picture: 4/ Collections

Niobium supports *collections*, which are another interesting way to organize and display your galleries of photos.

Collections are defined by a name and one or more directories with, optionally, regex filters for each one. Every time the gallery is (re)loaded, the collections are reindexed with the photos in the gallery that match their specific set of requirements (directories and filters). The structure of the subdirectories inside the indexed directories is preserved. A collection is accessed by appending its name to the main URL, like a virtual folder at the root of the gallery.

Unlike the main config file, the collections config file is reloaded when `.reload` is called, therefore Niobium doesn't need to be restarted when creating or modifying collections.

Note that subdirectories-specific passwords are not supported inside collections : collections will allow access to subdirectories that are password-protected in the main gallery. If this is not desired, sensitive subdirectories should be filtered out of the collection with the `FILTER_EXCLUDE` setting. It is possible, however, to set a global password on a collection with the `PASSWORD` setting in the definition of the collection (see below).

By default, the list of collections is displayed in the navigation panel when the URL points to the root of the gallery. This can be disabled through the `SHOW_COLLECTIONS_IN_NAVIGATION_PANEL` setting in the main config. Individual collections can also be hidden through the optional `HIDDEN` setting (see below).

The list of collections is defined in a dedicated configuration file, controlled by the `COLLECTIONS_FILE` parameter in the main config, which by default points to `niobium_collections.config`. The definition of a collection starts with `[[collection]]` followed by some settings :
- `NAME` _(mandatory)_ : the name of the collection, used in the URL. Only alphanumeric characters, dashes and underscores are allowed.
- `TITLE` _(optional)_ : the title of the collection, displayed in the navbar. If missing, `NAME` is used instead.
- `DESCRIPTION` _(optional)_ : some description for the collection, displayed in the navbar. Can contain HTML : be careful not to make editable by a user.
- `PASSWORD` _(optional)_ : an optional password required to access the collection.
- `HIDDEN` _(optional)_ : hide this collection from the navigation panel.
- `DIRS` _(mandatory)_ : the list of directories that should be included in this collection, each with the following settings :
  - `PATH` _(mandatory)_ : the path of the directory to include, relative to the root of the gallery (the `photos` folder). Set to `""` to include all photos in the gallery.
  - `FILTER` _(optional)_ : if specified, only the photos matching this regex will be included. The expression is checked against the full path of each photo, relative to the root of the gallery, for instance "`2023/March/Road trip/DSC_1975.jpg`".
  - `FILTER_EXCLUDE` _(optional)_ : if specified, each photo that should be included according to `FILTER` (ie all photos in this directory if `FILTER` is not set) is also checked against this filter which, if it matches, excludes the photo. This can be used to simplify the main filtering regex.

The syntax for the regex's can be found here : https://docs.rs/regex/latest/regex/#syntax. Remember that backslashes are used to escape characters from the configuration string, so to put an escaping backslash in the regex, you need to write two of them, for instance "`\\d`" -- and if you need to match against an actual, literal backslash, you need to write "`\\\\`". [I know, I know](https://xkcd.com/1638/).

### Example

`niobium_collections.config`

```
[[collection]]
NAME = "BestOf2022"
TITLE = "Best photos of 2022"
DESCRIPTION = "Some of my best shots. Please tell me which one you like best !"
DIRS = [
    { PATH = "2022/", FILTER = "\\/BestOf\\/", FILTER_EXCLUDE = "\\-private\\.jpg$" },
    { PATH = "Alex\\'s photos/" },
]

[[collection]]
NAME = "LatestHolidays"
TITLE = "My latest holidays !"
DIRS = [
    { PATH = "2023/July/Roma/", FILTER_EXCLUDE = "\\-private\\.jpg$" },
]

[[collection]]
NAME = "All_Nikon"
HIDDEN = true
PASSWORD = "eB0eixiu"
DIRS = [
    { PATH = "", FILTER = "\\/DSC_\\d+\\.jpg$" },
]
```

Let's say our gallery is hosted at https://photos.example.com/. This will create three collections :
- One that will be accessible through https://photos.example.com/BestOf2022/ that will index all the photos in the `2022` directory that have "`/BestOf/`" somewhere in their path (ie that are inside a `BestOf` subdirectory, such as "`2022/January/BestOf/DSC_1975.jpg`"), but excluding the photos that are specifically marked as private (ie that have a path that ends with "`-private.jpg`", for instance "`2022/January/BestOf/DSC_5840-private.jpg`"). This collection will also include all the photos in the "`Alex's photos/`" directory at the root of the gallery (without any filter because they are supposedly all good and public anyway).
- Another collection that will be accessible through https://photos.example.com/LatestHolidays/ and will show all the photos in "`2023/July/Roma/`" from the gallery, excluding the photos that are marked as private. Every year, you might change the configuration of this collection so that this public link always refer to the photos of your  latest holidays.
- A third on https://photos.example.com/All_Nikon/ that aggregates all photos in the gallery with a filename that looks like "`DSC_XXXX.jpg`" (why would you want to do this? I don't know, but it is possible). It will be hidden from the nav panel (only accessible using the link directly) and will be password-protected.

:bulb: **Tip** : a common use case would be to set a global password in the main config known only to you, and only share links to public collections curated to your specific needs. This allows more control over what photos gets included in the public galleries, and is useful to hide some of the internal directories structure of your `photos` folder. This also makes it easier to show a specific set of photos in an `iframe` embedded in an external webpage, that is constrained to a specific collection and doesn't allow the user to navigate up to the root of the gallery or see the other collections.



## :handshake: 5/ Acknowledgements

Main icon based on `panorama` by Font-Awesome : [https://fontawesome.com/icons/panorama?s=solid&f=classic](https://fontawesome.com/icons/panorama?s=solid&f=classic)

Backend built using the excellent Rust web framework Rocket :
- [https://rocket.rs/](https://rocket.rs/)
- [https://github.com/SergioBenitez/Rocket/](https://github.com/SergioBenitez/Rocket/)
