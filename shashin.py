from flask import Flask, render_template
import toml, os

app = Flask(__name__)
app.config.from_file('config.toml', load=toml.load)

def load_photos():
    filenames = [filename for filename in os.listdir(app.config['PHOTOS_DIR']) if filename.lower().endswith('.jpg') or filename.lower().endswith('.jpeg')];
    photos = []
    for filename in filenames:
        photos.append({
            'filename': filename
        })
    return photos

@app.route("/")
def hello_world():
    photos = load_photos()
    return render_template('main.html', photos=photos)
