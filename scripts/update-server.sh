SCRIPT_DIR=$( cd "$( dirname "$0" )" && pwd )
cd $SCRIPT_DIR
cd ..
echo "pwd: $(pwd)"
cargo build --release -p cc-server
sudo systemctl stop covert-connect
sudo cp ./target/release/cc-server /usr/local/bin/cc-server
sudo systemctl start covert-connect