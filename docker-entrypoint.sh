#!/bin/sh

# Check that the data directory exists
if [ ! -d /app/data ]; then
    >&2 echo "Error : the data directory doesn't exist, make sure to mount a persistent volume on /app/data"
    exit
fi

# Copy the sample config files to the data directory
if [ ! -f /app/data/niobium.config ]; then
    echo "Initializing data/niobium.config with a default config"
    cp /app/niobium.config.sample /app/data/niobium.config
fi
if [ ! -f /app/data/niobium_collections.config.sample ]; then
    cp /app/niobium_collections.config.sample /app/data/niobium_collections.config.sample
fi

# Set default values for the environment variables to point to the data directory
export NIOBIUM_CONFIG_FILE="${NIOBIUM_CONFIG_FILE:-/app/data/niobium.config}"
export NIOBIUM_SECRET_FILE="${NIOBIUM_SECRET_FILE:-/app/data/.secret}"
export NIOBIUM_CACHE_DIR="${NIOBIUM_CACHE_DIR:-/app/data/cache}"
export NIOBIUM_DATABASE_PATH="${NIOBIUM_DATABASE_PATH:-/app/data/niobium.sqlite}"
export NIOBIUM_COLLECTIONS_FILE="${NIOBIUM_COLLECTIONS_FILE:-/app/data/niobium_collections.config}"

# Set the default address to listen to the outside of the container instead of localhost
export NIOBIUM_ADDRESS="${NIOBIUM_ADDRESS:-0.0.0.0}"

exec "$@"