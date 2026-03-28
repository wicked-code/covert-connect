#!/bin/bash

# Check if the script is run as root
if [ "$EUID" -ne 0 ]
  then echo "Please run as root (use sudo)"
  exit
fi

SCRIPT_DIR=$( cd "$( dirname "$0" )" && pwd )

cp "$SCRIPT_DIR/../conf/covert-connect.service" "/etc/systemd/system/covert-connect.service"

echo "Reloading systemd daemon..."
systemctl daemon-reload

echo "Enabling covert-connect.service to start on boot..."
systemctl enable covert-connect
