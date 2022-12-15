# Niobium


<div align="center"><img src="static/img/niobium.svg" width="200"></div>

### <div align="center">Modern, high-performance, web-based photo gallery viewer</div>

### <div align="center">:framed_picture: :sparkles: :rocket:</div>


![Screenshot](https://user-images.githubusercontent.com/2955191/207928165-247f39c1-826a-481d-a7c0-08a3a804d301.jpg)

### <div align="center">[:fire: Demo](https://photo.silica.io/)</div>

- [1/ Installation](#hammer_and_wrench-1-installation)
  - [1.1/ (Option A) Install using a release](#11-option-a-install-using-a-release-recommended)
  - [1.1/ (Option B) Build from source](#11-option-b-build-from-source)
  - [1.2/ Start as a daemon](#12-start-as-a-daemon)
  - [1.3/ Set up the reverse proxy](#13-set-up-the-reverse-proxy)
- [2/ Configuration](#gear-2-configuration)
  - [2.1/ Main config file](#21-main-config-file)
  - [2.2/ Subdirectories config files](#22-subdirectories-config-files)

## :hammer_and_wrench: 1/ Installation

### 1.1/ *(Option A)* Install using a release (recommended)

Example of how to install Niobium on Debian 11 in `/var/www/my_photos`, customize the paths as needed.

```
# cd /var/www/
# wget https://github.com/Foalyy/niobium/releases/download/v0.2.0/niobium_0.2.0.zip
# unzip niobium_0.2.0.zip
# mv niobium_0.2.0 my_photos
# cd my_photos
```

Copy your photos to the `photos/` directory here. Alternatively, you can leave your photos folder where it currently is and set the `PHOTOS_DIR` setting to point to that path (see below).

```
# cp -r /path/to/my/photos ./photos
```

Take a look at the config file, every setting is documented inside :

```
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

Niobium is now running, but for a more permanent installation, keep reading [section 1.2](#12-start-as-a-daemon).

### 1.1/ *(Option B)* Build from source

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


### 1.2/ Start as a daemon

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
journalctl -u niobium.service -f
```

If your OS is not `systemd`-based or your religion forbids you from using `systemd`, adapt the daemon config file accordingly.


### 1.3/ Set up the reverse proxy

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


## :gear: 2/ Configuration

### 2.1/ Main config file

The main config file, `niobium.config`, is self-documented, see below.

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


### 2.2/ Subdirectories config files

When your photos folder is sorted into subdirectories, some settings can be customized for specific directories by creating a `.niobium.config` config file (note the leading dot in the filename) inside these directories. Available settings are those marked as "overridable" in the main config file.

For example, you can :
- ignore a certain subdirectory completely by setting `INDEX = false`
- hide a subdirectory from the navigation panel by setting `HIDDEN = true`
- specify a password required to access a subdirectory by setting `PASSWORD = "1337P455W0RD"`
