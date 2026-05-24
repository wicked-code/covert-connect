#!/bin/bash

# Check if the script is run as root
if [ "$EUID" -ne 0 ]
  then echo "Please run as root (use sudo)"
  exit
fi

SCRIPT_DIR=$( cd "$( dirname "$0" )" && pwd )
mkdir -p "/etc/covert-connect"
cp "$SCRIPT_DIR/../conf/default_server.yaml" "/etc/covert-connect/server.yaml"

cd $SCRIPT_DIR
cd ..
echo "pwd: $(pwd)"
cargo build -p cc-server --profile release-prod

./target/release-prod/cc-server -n -c "/etc/covert-connect/server.yaml"

sudo cp ./target/release-prod/cc-server /usr/local/bin/cc-server
