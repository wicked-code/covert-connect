SCRIPT_DIR=$( cd "$( dirname "$0" )" && pwd )
cd $SCRIPT_DIR
cd ..
echo "pwd: $(pwd)"
cargo build -p cc-server --profile release-prod
sudo systemctl stop covert-connect
sudo cp ./target/release-prod/cc-server /usr/local/bin/cc-server
sudo systemctl start covert-connect
