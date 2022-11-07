import os, random, toml, sqlite3
from flask import Flask, current_app, g, render_template, abort, send_from_directory
import werkzeug
from wand.image import Image
from pprint import pprint


### Internal constants
UID_LENGTH = 10


### Database

# Get a reference to the singleton database instance, and create the schema if needed
def get_db():
    if 'db' not in g:
        g.db = sqlite3.connect('shashin.sqlite', detect_types=sqlite3.PARSE_DECLTYPES)
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


### Photos

# Load the photos from the filesystem and sync it with the database
def load_photos():
    # Get the list of photos currently saved in the database
    def get_photos_from_db(db):
        cur.execute("SELECT * FROM photo ORDER BY id DESC")
        return [{key: row[key] for key in row.keys()} for row in cur.fetchall()]

    # Generate a UID that is guaranteed to not already exist in the provided list
    def generate_uid(existing_uids):
        # List of available characters (biased toward numbers)
        chars = "012345678901234567890123456789abcdefghijklmnopqrstuvwxyz"

        while True:
            # Generate a UID
            uid = ''.join([random.choice(chars) for i in range(UID_LENGTH)])

            # Check that it doesn't already exist before returning it, otherwise loop to generate another one
            if uid not in existing_uids:
                return uid

    # List the files inside the photos directory
    filenames = sorted([filename for filename in os.listdir(app.config['PHOTOS_DIR']) if filename.lower().endswith('.jpg') or filename.lower().endswith('.jpeg')]);

    with get_db() as db:
        cur = g.db.cursor()

        # Get the list of photos saved in the database
        photos_in_db = get_photos_from_db(db)

        # Find photos in the filesystem that are not yet in the database and insert them
        photos_to_insert = [filename for filename in filenames if filename not in [photo['filename'] for photo in photos_in_db]]
        if photos_to_insert:
            rows_to_insert = []
            existing_uids = [photo['uid'] for photo in photos_in_db]
            for filename in photos_to_insert:
                uid = generate_uid(existing_uids)
                rows_to_insert.append((filename, uid, '.'.join(filename.split('.')[:-1])))
                existing_uids.append(uid)
            print(f"Inserting {len(rows_to_insert)} photo(s) in the database : {', '.join(photos_to_insert)}")
            cur.executemany("INSERT INTO photo(filename, uid, title) VALUES (?,?,?)", rows_to_insert)

        # Find photos in the database that are not in the filesystem anymore, and delete them
        photos_to_remove = [{'uid': photo['uid'], 'filename': photo['filename']} for photo in photos_in_db if photo['filename'] not in filenames]
        if photos_to_remove:
            print(f"Removing {len(photos_to_remove)} photo(s) from the database : {', '.join([photo['filename'] for photo in photos_to_remove])}")
            cur.executemany("DELETE FROM photo WHERE uid=?", [(photo['uid'],) for photo in photos_to_remove])

        # If there were photos inserted or deleted, reload the updated list from the database
        if photos_to_insert or photos_to_remove:
            photos_in_db = get_photos_from_db(db)

    return photos_in_db


### App
app = Flask(__name__)
app.config.from_file('config.toml', load=toml.load)
for dir_name in ['PHOTOS_DIR', 'CACHE_DIR']:
    if not app.config[dir_name].endswith('/'):
        app.config[dir_name] += '/'
app.teardown_appcontext(close_db)



@app.route("/")
def gallery():
    photos = load_photos()
    return render_template('main.html', photos=photos)

@app.route("/<uid>")
def get_photo(uid):
    # Get the filename associated to this uid
    with get_db() as db:
        cur = g.db.cursor()
        cur.execute("SELECT filename FROM photo WHERE uid=?", (uid,))
        photo = cur.fetchone()
    if photo is None:
        abort(404)

    # Return the photo
    return send_from_directory(app.config['PHOTOS_DIR'], photo['filename'])

@app.route("/<uid>/thumbnail")
def get_thumbnail(uid):
    # Return the thumbnail from the cache folder if it exists
    thumbnail_filename = 'thumbnail_' + uid + '.jpg'
    try:
        return send_from_directory(app.config['CACHE_DIR'], thumbnail_filename)

    except werkzeug.exceptions.NotFound as e:
        # The thumbnail doesn't exist, try to generate it

        # Get the filename associated to this uid
        with get_db() as db:
            cur = g.db.cursor()
            cur.execute("SELECT filename FROM photo WHERE uid=?", (uid,))
            row = cur.fetchone()
        if row is None:
            abort(404)
        filename = row['filename']

        # Resize the image and save it to the cache directory
        photo = Image(filename = app.config['PHOTOS_DIR'] + filename)
        max_size = app.config['THUMBNAIL_MAX_SIZE']
        if photo.width > max_size or photo.height > max_size:
            # Find the best ratio to make the image fit into the max dimension
            resized_width = max_size
            resize_ratio = photo.width / max_size
            resized_height = photo.height / resize_ratio
            if resized_height > max_size:
                resize_ratio = photo.height / max_size
                resized_height = max_size
                resized_width = photo.width / resize_ratio
            photo.resize(round(resized_width), round(resized_height))
        photo.save(filename = app.config['CACHE_DIR'] + thumbnail_filename)
        print(f"Thumbnail for {filename} generated in the cache directory")
        return send_from_directory(app.config['CACHE_DIR'], thumbnail_filename)

@app.errorhandler(404)
def page_not_found(error):
    return render_template('404.html'), 404