import multiprocessing

bind = "127.0.0.1:8000"
workers = multiprocessing.cpu_count() * 2 + 1
#accesslog = "/var/log/gunicorn_niobium_access.log"
#errorlog = "/var/log/gunicorn_niobium_error.log"
#capture_output = True
