from flask import Flask, render_template, current_app, g
import os, random, toml, sqlite3
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
app.teardown_appcontext(close_db)



@app.route("/")
def gallery():
    photos = load_photos()
    return render_template('main.html', photos=photos)
