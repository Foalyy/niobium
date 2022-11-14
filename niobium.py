import os, sys, shutil, random, toml, sqlite3, hashlib
from flask import Flask, request, current_app, g, render_template, abort, send_from_directory, stream_with_context
from werkzeug.middleware.proxy_fix import ProxyFix
import werkzeug
from wand.image import Image
from wand.exceptions import CorruptImageError


### Internal constants
UID_CHARS = "012345678901234567890123456789abcdefghijklmnopqrstuvwxyz" # Intentionally biased toward numbers
EXIF_METADATA_MAPPING = {
    'exif:DateTimeDigitized': 'date_taken',
    'exif:DateTimeOriginal': 'date_taken',
    'exif:Model': 'camera_model',
    'exif:LensModel': 'lens_model',
    'exif:FocalLength': 'focal_length',
    'exif:FNumber': 'aperture',
    'exif:ExposureTime': 'exposure_time',
    'exif:PhotographicSensitivity': 'sensitivity',
}



### App

# Create the main app object
app = Flask(__name__)

# Read the config file
app.config.from_file('config.toml', load=toml.load)

# Check directories specified in config
for dir_name in ['PHOTOS_DIR', 'CACHE_DIR']:
    # Make sure the path ends with a '/'
    if not app.config[dir_name].endswith('/'):
        app.config[dir_name] += '/'

# If running behind a reverse proxy, tell Flask to use the X-Forwarded headers
# See https://flask.palletsprojects.com/en/2.2.x/deploying/proxy_fix/
if app.config['BEHIND_REVERSE_PROXY']:
    app.wsgi_app = ProxyFix(
        app.wsgi_app, x_for=1, x_proto=1, x_host=1, x_prefix=1
    )


### Database

# Get a reference to the singleton database instance, and create the schema if needed
def get_db():
    if 'db' not in g:
        g.db = sqlite3.connect(app.config['DATABASE_PATH'], detect_types=sqlite3.PARSE_DECLTYPES)
        g.db.row_factory = sqlite3.Row
        try:
            g.db.cursor().execute("SELECT id FROM photo LIMIT 1;")
        except sqlite3.OperationalError as e:
            print("Database is empty, creating schema")
            with current_app.open_resource('schema.sql') as f:
                g.db.executescript(f.read().decode('utf8'))
    return g.db

# Close the database connection
def close_db(e=None):
    db = g.pop('db', None)
    if db is not None:
        db.close()
app.teardown_appcontext(close_db)


### Photos

# Return the list of valid subdirectories in the given path in the photos folder
def list_subdirs(path):
    if not path.startswith('/'):
        path = '/' + path
    subdirs = []
    with os.scandir(app.config['PHOTOS_DIR'][:-1] + path) as directory:
        for entry in directory:
            if entry.is_dir() and not entry.name.startswith('.'):
                subdirs.append(entry.name)
    return subdirs

# Generate a UID that is guaranteed to not already exist in the provided list
def generate_uid(existing_uids):
    while True:
        # Generate a UID
        uid = ''.join([random.choice(UID_CHARS) for i in range(app.config['UID_LENGTH'])])

        # Check that it doesn't already exist before returning it, otherwise loop to generate another one
        if uid not in existing_uids:
            return uid

# Calculate and return the MD5 hash of the given file
def calculate_file_md5(filepath):
    with open(filepath, 'rb') as file:
        return hashlib.md5(file.read()).hexdigest()

# Load the photos from the filesystem and sync them with the database
def load_photos(path):
    # Inner function used to load photos recursively
    def _load_photos(path, displayed_photos, existing_uids, photos_to_insert, photos_to_remove, paths_found, is_subdir):
        # Make sure this path is formatted correctly and append it to the list of paths found
        if not path.endswith('/'):
            path += '/'
        paths_found.append(path)

        # List the files inside this path in the photos directory
        filenames_in_fs = []
        with os.scandir(app.config['PHOTOS_DIR'][:-1] + path) as directory:
            for entry in directory:
                if entry.is_file() and (entry.name.lower().endswith('.jpg') or entry.name.lower().endswith('.jpeg')) and not entry.name.startswith('.'):
                    filenames_in_fs.append(entry.name)
        filenames_in_fs.sort()

        with get_db() as db:
            cur = db.cursor()

            # Get the list of photos saved in the database for this path
            sql = "SELECT * FROM photo WHERE path=:path ORDER BY " + ', '.join([clause + " " + ("ASC", "DESC")[app.config['REVERSE_SORT_ORDER']] for clause in app.config['SORT_ORDER'].split(',')])
            cur.execute(sql, {'path': path})
            photos_in_db = [{key: row[key] for key in row.keys()} for row in cur.fetchall()]

            # Find photos in the filesystem that are not in the database yet
            filenames_in_db = [photo['filename'] for photo in photos_in_db]
            photos_to_insert += [{'path': path, 'filename': filename} for filename in filenames_in_fs if filename not in filenames_in_db]

            # Find photos in the database that are not in the filesystem anymore
            photos_to_remove += [{'uid': photo['uid'], 'path': path, 'filename': photo['filename'], 'md5': photo['md5']} for photo in photos_in_db if photo['filename'] not in filenames_in_fs]

            # Delete old resized photos from cache
            resized_photos_to_delete = []
            all_uids_in_path = [photo['uid'] for photo in photos_in_db]
            suffix = '.jpg'
            for prefix in ['thumbnail_', 'large_']:
                try:
                    resized_photos = [filename for filename in os.listdir(app.config['CACHE_DIR'][:-1] + path) if filename.lower().startswith(prefix) and filename.lower().endswith(suffix)];
                except FileNotFoundError:
                    resized_photos = []
                for resized_photo in resized_photos:
                    uid = resized_photo[len(prefix): -len(suffix)]
                    if not uid in all_uids_in_path:
                        resized_photos_to_delete.append(resized_photo)
            if resized_photos_to_delete:
                print(f"Deleting {len(resized_photos_to_delete)} obsolete resized photos in \"{path}\" from cache : {', '.join(resized_photos_to_delete)}")
                for resized_photo in resized_photos_to_delete:
                    os.remove(app.config['CACHE_DIR'][:-1] + path + resized_photo)

        # If this is a subdirectory, add these photos only if the SHOW_PHOTOS_FROM_SUBDIRS config is enabled
        if not is_subdir or app.config['SHOW_PHOTOS_FROM_SUBDIRS']:
            # Filter out hidden photos
            displayed_photos += [photo for photo in photos_in_db if not photo['hidden']]

        # If the INDEX_SUBDIRS config is enabled, recursively load photos from subdirectories
        if app.config['INDEX_SUBDIRS']:
            # Find the list of subdirectories in the path, in the filesystem
            subdirs = list_subdirs(path)

            # Clean obsolete subdirectories (that do not correspond to a subdirectory in the photos folder) from the cache folder
            subdirs_in_cache = []
            try:
                with os.scandir(app.config['CACHE_DIR'][:-1] + path) as directory:
                    for entry in directory:
                        if entry.is_dir():
                            subdirs_in_cache.append(entry.name)
            except FileNotFoundError:
                pass
            for subdir in subdirs_in_cache:
                if subdir not in subdirs:
                    try:
                        shutil.rmtree(app.config['CACHE_DIR'][:-1] + path + subdir)
                    except Exception as e:
                        print(f"Error: unable to remove a directory from cache : \"{app.config['CACHE_DIR'][:-1] + path + subdir}\", {e}", file=sys.stderr)
                        pass

            # Load subdirs recursively
            if subdirs:
                subdirs.sort()
                for subdir in subdirs:
                    displayed_photos = _load_photos(path + subdir, displayed_photos, existing_uids, photos_to_insert, photos_to_remove, paths_found, True)

        return displayed_photos

    # Create the main directories if they don't exist
    for dir_name in ['PHOTOS_DIR', 'CACHE_DIR']:
        if not os.path.isdir(app.config[dir_name]):
            print("Creating empty directory " + app.config[dir_name])
            os.makedirs(app.config[dir_name])

    # Get all existing UIDs from the database
    with get_db() as db:
        cur = db.cursor()
        cur.execute("SELECT uid FROM photo")
        existing_uids = [row['uid'] for row in cur.fetchall()]

    # Load the photos in this path
    displayed_photos = []
    photos_to_insert = []
    photos_to_remove = []
    paths_found = []
    displayed_photos = _load_photos(path, displayed_photos, existing_uids, photos_to_insert, photos_to_remove, paths_found, False)

    # Get the list of all known subdirs of the current path in the database, check if some have been removed, and if so add their photos to the 'to_remove' list
    if app.config['INDEX_SUBDIRS']:
        deleted_paths = []
        with get_db() as db:
            cur = db.cursor()
            cur.execute("SELECT path FROM photo WHERE SUBSTR(path, 1, ?)=? GROUP BY path;", (len(path), path))
            known_paths_in_db = [row['path'] for row in cur.fetchall()]
        for known_path in known_paths_in_db:
            if known_path not in paths_found:
                deleted_paths.append(known_path)
        if deleted_paths:
            with get_db() as db:
                cur = db.cursor()
                cur.execute("SELECT filename, md5, path, uid FROM photo WHERE path IN (" + ','.join(['?']*len(deleted_paths)) + ");", (deleted_paths))
                photos_to_remove += [{'uid': row['uid'], 'path': row['path'], 'filename': row['filename'], 'md5': row['md5']} for row in cur.fetchall()]

    # Calculate the MD5 hashes of the new files
    for photo in photos_to_insert:
        photo['md5'] = calculate_file_md5(app.config['PHOTOS_DIR'][:-1] + photo['path'] + photo['filename'])

    # Detect if some of the insert/remove are actually the same file that has been moved or renamed
    photos_to_move = []
    if photos_to_insert and photos_to_remove:
        for new_photo in photos_to_insert:
            for old_photo in photos_to_remove:
                if old_photo['md5'] == new_photo['md5']:
                    photos_to_move.append({'old': old_photo, 'new': new_photo})
                    break
        for moved_photo in photos_to_move:
            photos_to_insert.remove(moved_photo['new'])
            photos_to_remove.remove(moved_photo['old'])

    # Apply detected modifications (photos added, moved, or deleted) to the database
    if photos_to_insert:
        rows_to_insert = []
        keys = ['filename', 'path', 'uid', 'md5']
        for photo in photos_to_insert:
            # Generate a new UID for this photo
            photo['uid'] = generate_uid(existing_uids)
            existing_uids.append(photo['uid'])

            rows_to_insert.append({key: photo[key] for key in keys})

        print(f"Inserting {len(rows_to_insert)} photo(s) in the database : " + ', '.join(['"' + photo['path'] + photo['filename'] + '"' for photo in photos_to_insert]))
        with get_db() as db:
            db.cursor().executemany(f"INSERT INTO photo({', '.join(keys)}) VALUES ({', '.join([':' + key for key in keys])})", rows_to_insert)
    if photos_to_remove:
        print(f"Removing {len(photos_to_remove)} photo(s) from the database : " + ', '.join(['"' + photo['path'] + photo['filename'] + '"' for photo in photos_to_remove]))
        with get_db() as db:
            db.cursor().executemany("DELETE FROM photo WHERE uid=?", [(photo['uid'],) for photo in photos_to_remove])
    if photos_to_move:
        print(f"Renaming/moving {len(photos_to_move)} photo(s) from the database : " + ', '.join(['"' + photo['old']['path'] + photo['old']['filename'] + '"->"' + photo['new']['path'] + photo['new']['filename'] + '"' for photo in photos_to_move]))
        with get_db() as db:
            db.cursor().executemany("UPDATE photo SET filename=:filename, path=:path WHERE uid=:uid", [{'filename': photo['new']['filename'], 'path': photo['new']['path'], 'uid': photo['old']['uid']} for photo in photos_to_move])

    # If there were some modifications to the photos, reload the database after updating it
    if photos_to_insert or photos_to_remove or photos_to_move:
        displayed_photos = _load_photos(path, [], existing_uids, [], [], [], False)

    # Add an index to the photos
    for index, photo in enumerate(displayed_photos):
        photo['index'] = index

    return displayed_photos


# Calculate the dimensions of each photo in the given list and persist them to the database
def calc_photos_dimensions(photos):
    rows = []
    for photo in photos:
        if photo['width'] == None or photo['height'] == None:
            print(f"Calculating dimensions of photo {photo['filename']}...")
            row = {
                'uid': photo['uid'],
                'width': 0,
                'height': 0,
            }
            try:
                with Image(filename = app.config['PHOTOS_DIR'] + photo['path'][1:] + photo['filename']) as image:
                    # Image dimensions
                    row['width'] = image.width
                    photo['width'] = image.width
                    row['height'] = image.height
                    photo['height'] = image.height
                rows.append(row)
            except CorruptImageError as e:
                print(f"Photo \"{photo['path'][1:] + photo['filename']}\" is corrupted : {e}", file=sys.stderr)

    if rows:
        with get_db() as db:
            cur = db.cursor()
            cur.executemany("UPDATE photo SET width=:width, height=:height WHERE uid=:uid", rows)


# Extract useful informations from the given photo and persist them to the database
def parse_photo_metadata(photo):
    if photo['metadata_parsed']:
        # Metadata already parsed, nothing to do
        return

    print(f"Parsing metadata for photo {photo['filename']}...")
    row = {
        'uid': photo['uid'],
        'width': 0,
        'height': 0,
        'color': '',
        'date_taken': '',
        'camera_model': '',
        'lens_model': '',
        'focal_length': '',
        'aperture': '',
        'exposure_time': '',
        'sensitivity': '',
    }
    try:
        with Image(filename = app.config['PHOTOS_DIR'] + photo['path'][1:] + photo['filename']) as image:
            # Image dimensions
            row['width'] = image.width
            row['height'] = image.height

            # Compute the photo's average color
            average_color = [image.mean_channel(channel)[0] / image.quantum_range for channel in ['red', 'green', 'blue']]
            row['color'] = ''.join(['{:02x}'.format(int(channel_value * 255 / 6)) for channel_value in average_color])

            # Parse EXIF metadata
            if app.config['READ_EXIF']:
                for exif_key, db_key in EXIF_METADATA_MAPPING.items():
                    if exif_key in image.metadata:
                        try:
                            value = image.metadata[exif_key]
                            if db_key in ['focal_length', 'aperture'] and '/' in value:
                                value = value.split('/')
                                value = str(round(float(value[0]) / float(value[1]), len(value[1])))
                        except Exception as e:
                            print(e)
                        row[db_key] = value
        with get_db() as db:
            cur = db.cursor()
            cur.execute("UPDATE photo SET metadata_parsed=1, " + ', '.join([f"{key}=:{key}" for key in row if key != 'uid']) + " WHERE uid=:uid", row)
    except CorruptImageError as e:
        print(f"Photo \"{photo['path'][1:] + photo['filename']}\" is corrupted : {e}", file=sys.stderr)

# Load a photo entry from the database based on the given UID
def get_photo_from_uid(uid):
    # Get the filename associated to this uid
    with get_db() as db:
        cur = db.cursor()
        cur.execute("SELECT * FROM photo WHERE uid=:uid", {'uid': uid})
        photo = cur.fetchone()
    if photo is None or photo['hidden']:
        abort(404)

    # Parse metadata if not done already
    if not photo['metadata_parsed']:
        parse_photo_metadata(photo)

    return photo

# Get a Response returning the file for the resized version of a photo based on the given UID, after generating it if needed
def get_resized_photo(uid, prefix, max_size, quality):
    photo = get_photo_from_uid(uid)
    path = app.config['CACHE_DIR'][:-1] + photo['path']
    try:
        os.makedirs(path)
    except FileExistsError:
        pass

    # Return the resized photo from the cache folder if it exists
    resized_filename = prefix + '_' + uid + '.jpg'
    try:
        return send_from_directory(path, resized_filename)

    except werkzeug.exceptions.NotFound as e:
        # This resized version doesn't exist, try to generate it
        try:
            image = Image(filename = app.config['PHOTOS_DIR'][:-1] + photo['path'] + photo['filename'])
            max_size = max_size
            if image.width > max_size or image.height > max_size:
                # Find the best ratio to make the image fit into the max dimension
                resized_width = max_size
                resize_ratio = image.width / max_size
                resized_height = image.height / resize_ratio
                if resized_height > max_size:
                    resize_ratio = image.height / max_size
                    resized_height = max_size
                    resized_width = image.width / resize_ratio
                image.resize(round(resized_width), round(resized_height))
            image.compression_quality = quality
            image.save(filename = path + resized_filename)
            print(f"Resized version ({prefix}) of \"{photo['path'][1:] + photo['filename']}\" generated in the cache directory")
        except CorruptImageError as e:
            print(f"Photo \"{photo['path'][1:] + photo['filename']}\" is corrupted : {e}", file=sys.stderr)
        return send_from_directory(path, resized_filename)

def check_path(path):
    # Make sure path if formatted correctly
    if not path.startswith('/'):
        path = '/' + path
    if not path.endswith('/'):
        path += '/'

    # Forbid opening subdirectories if INDEX_SUBDIRS is disabned
    if path != '/' and not app.config['INDEX_SUBDIRS']:
        abort(404)
        return False

    # Prevent path traversal attacks
    if not os.path.commonprefix([app.config['PHOTOS_DIR'], app.config['PHOTOS_DIR'] + path]):
        abort(404)
        return False
    
    # Make sure this directory exists in the filesystem and that none of the directories in the path starts with a dot (hidden directories)
    if not os.path.isdir(app.config['PHOTOS_DIR'] + path) or True in [p.startswith('.') for p in path.split('/')]:
        abort(404)
        return False

    return path



### Routes

@app.route("/.grid")
def get_grid_root():
    return get_grid('/')

@app.route("/<path:path>/.grid")
def get_grid(path):
    path = check_path(path)
    if path:
        # Load the photos from this path
        photos = load_photos(path)

        # Only return a subset if requested
        n_photos = len(photos)
        start = 0
        count = n_photos
        try:
            start = int(request.args.get('start', start))
            if start < 0 or start >= n_photos:
                start = 0
            count = int(request.args.get('count', count))
            if count < 0 or count > n_photos:
                count = n_photos
        except:
            pass
        if start != 0 or count != n_photos:
            photos = photos[start:start+count]

        # Only return a single UID if requested
        requestedUID = request.args.get('uid', None)
        if requestedUID:
            photos = [photo for photo in photos if photo['uid'] == requestedUID]

        # If the requested set is small enough, calculate the image sizes to improve the first display
        if len(photos) <= 100:
            calc_photos_dimensions(photos)

        return render_template('grid.html', photos=photos, n_photos=n_photos)

@app.route("/")
def get_gallery_root():
    return get_gallery('/')

@app.route("/<path:path>/")
def get_gallery(path):
    path = check_path(path)
    if path:
        # Navigation panel
        path_split = [d for d in path.split('/') if d]
        nav = {
            'is_root': path == '/',
            'path_current': path,
            'path_parent': '/' + '/'.join(path_split[:-1]) + '/',
            'path_split': path_split,
            'subdirs': sorted(list_subdirs(path)) if app.config['SHOW_NAVIGATION_PANEL'] else [],
        }

        return render_template('main.html', nav=nav, uid_chars=''.join(sorted(set(UID_CHARS))))

@app.route("/<uid>/grid-item")
def get_grid_item(uid):
    photo = get_photo_from_uid(uid)
    return render_template('grid-item.html', photo=photo)

@app.route("/<uid>")
def get_photo(uid):
    photo = get_photo_from_uid(uid)
    return send_from_directory(app.config['PHOTOS_DIR'] + photo['path'][1:], photo['filename'])

@app.route("/<uid>/thumbnail")
def get_thumbnail(uid):
    return get_resized_photo(uid, prefix='thumbnail', max_size=app.config['THUMBNAIL_MAX_SIZE'], quality=app.config['THUMBNAIL_QUALITY'])

@app.route("/<uid>/large")
def get_large(uid):
    return get_resized_photo(uid, prefix='large', max_size=app.config['LARGE_VIEW_MAX_SIZE'], quality=app.config['LARGE_VIEW_QUALITY'])

@app.route("/<uid>/download")
def download_photo(uid):
    photo = get_photo_from_uid(uid)
    download_name = app.config['DOWNLOAD_PREFIX'] + photo['uid'] + '.jpg'
    return send_from_directory(app.config['PHOTOS_DIR'] + photo['path'][1:], photo['filename'], as_attachment=True, download_name=download_name)

@app.errorhandler(404)
def page_not_found(error):
    return render_template('404.html'), 404