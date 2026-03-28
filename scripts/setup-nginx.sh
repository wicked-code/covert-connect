#!/usr/bin/env bash
set -euo pipefail

if command -v nginx >/dev/null 2>&1; then
	echo "nginx is already installed."
else
	echo "nginx is not installed. Installing..."

	if command -v apt-get >/dev/null 2>&1; then
		sudo apt-get update
		sudo apt-get install -y nginx
	elif command -v dnf >/dev/null 2>&1; then
		sudo dnf install -y nginx
	elif command -v yum >/dev/null 2>&1; then
		sudo yum install -y nginx
	elif command -v pacman >/dev/null 2>&1; then
		sudo pacman -Sy --noconfirm nginx
	elif command -v zypper >/dev/null 2>&1; then
		sudo zypper --non-interactive install nginx
	else
		echo "No supported package manager found. Please install nginx manually."
		exit 1
	fi

	echo "nginx installation complete."
fi

SCRIPT_DIR=$( cd "$( dirname "$0" )" && pwd )
cd $SCRIPT_DIR
cd ..
echo "pwd: $(pwd)"
cargo build --release -p cc-server

NGINX_BLOCK=$(./target/release/cc-server -u -c /etc/covert-connect/server.yaml 2>&1 | awk 'found { print } /nginx config example:/ { found=1 }')

if [ -z "$NGINX_BLOCK" ]; then
	echo "Failed to extract nginx config block from cc-server output."
	exit 1
fi

echo "Generated nginx location block:"
echo "$NGINX_BLOCK"

LOCATION_PATH=$(printf '%s\n' "$NGINX_BLOCK" | awk '/^[[:space:]]*location[[:space:]]+[^[:space:]]+[[:space:]]*\{/{print $2; exit}')
if [ -z "$LOCATION_PATH" ]; then
	echo "Could not parse location path from generated nginx block."
	exit 1
fi

LOCATION_PATH_REGEX=$(printf '%s\n' "$LOCATION_PATH" | sed 's/[][\\.^$*+?(){}|]/\\&/g')
GENERATED_LOCATION_PATH_REGEX='^/[A-Za-z0-9_-]{33,}$'

if [ -f /etc/nginx/sites-available/default ]; then
	NGINX_DEFAULT_CONF=/etc/nginx/sites-available/default
elif [ -f /etc/nginx/conf.d/default.conf ]; then
	NGINX_DEFAULT_CONF=/etc/nginx/conf.d/default.conf
else
	echo "Could not find nginx default server config file."
	exit 1
fi

TMP_BLOCK=$(mktemp)
TMP_NEW_CONF=$(mktemp)
trap 'rm -f "$TMP_BLOCK" "$TMP_NEW_CONF"' EXIT

printf '%s\n' "$NGINX_BLOCK" > "$TMP_BLOCK"

# Insert the generated location block into the first server block and replace
# any previously generated location block whose path matches /<base64url>.
sudo awk -v block_file="$TMP_BLOCK" -v generated_location_path_regex="$GENERATED_LOCATION_PATH_REGEX" '
function brace_delta(s, open_count, close_count) {
	open_count = gsub(/\{/, "{", s)
	close_count = gsub(/\}/, "}", s)
	return open_count - close_count
}
function location_path_from_line(s, stripped, fields) {
	stripped = s
	sub(/^[[:space:]]*location[[:space:]]+/, "", stripped)
	split(stripped, fields, /[[:space:]]+/)
	return fields[1]
}
BEGIN {
	in_server = 0
	server_depth = 0
	skip_block = 0
	skip_depth = 0
	inserted = 0
}
{
	line = $0

	if (skip_block) {
		skip_depth += brace_delta(line)
		if (skip_depth <= 0) {
			skip_block = 0
		}
		next
	}

	if (!in_server && line ~ /^[[:space:]]*server[[:space:]]*\{[[:space:]]*$/) {
		in_server = 1
		server_depth = 1
		print line
		next
	}

	if (in_server) {
		if (line ~ /^[[:space:]]*location[[:space:]]+[^[:space:]]+[[:space:]]*\{/) {
			location_path = location_path_from_line(line)
			if (location_path ~ generated_location_path_regex) {
				skip_block = 1
				skip_depth = 1
				next
			}
		}

		if (server_depth == 1 && line ~ /^[[:space:]]*}[[:space:]]*$/ && inserted == 0) {
			print ""
			while ((getline bline < block_file) > 0) {
				print "    " bline
			}
			close(block_file)
			inserted = 1
		}

		print line
		server_depth += brace_delta(line)
		if (server_depth <= 0) {
			in_server = 0
		}
		next
	}

	print line
}
END {
	if (inserted == 0) {
		exit 2
	}
}
' "$NGINX_DEFAULT_CONF" > "$TMP_NEW_CONF"

LOCATION_COUNT=$(grep -Ec "^[[:space:]]*location[[:space:]]+$LOCATION_PATH_REGEX[[:space:]]*\\{" "$TMP_NEW_CONF")
if [ "$LOCATION_COUNT" -ne 1 ]; then
	echo "Expected exactly one location block for $LOCATION_PATH in $NGINX_DEFAULT_CONF, found $LOCATION_COUNT."
	exit 1
fi

sudo cp "$TMP_NEW_CONF" "$NGINX_DEFAULT_CONF"
echo "Updated $NGINX_DEFAULT_CONF with generated location block."

sudo nginx -t
if command -v systemctl >/dev/null 2>&1; then
	sudo systemctl reload nginx
else
	sudo nginx -s reload
fi

echo "nginx config reloaded successfully."

# --- SSL setup ---

# Exit if SSL is already actively configured (non-commented ssl_certificate directive)
if grep -Eq "^[[:space:]]*ssl_certificate[[:space:]]" "$NGINX_DEFAULT_CONF"; then
	echo "SSL is already actively configured in $NGINX_DEFAULT_CONF. Skipping SSL setup."
	exit 0
fi

# Ask for domain name
read -rp "Enter your domain name for SSL setup (leave blank to skip): " DOMAIN || true
if [ -z "$DOMAIN" ]; then
	echo "No domain provided. Skipping SSL setup."
	exit 0
fi

# Determine public IP of this host
PUBLIC_IP=$(
	curl -s --max-time 5 https://api.ipify.org 2>/dev/null ||
	curl -s --max-time 5 https://ifconfig.me 2>/dev/null ||
	curl -s --max-time 5 https://icanhazip.com 2>/dev/null ||
	true
)
if [ -z "$PUBLIC_IP" ]; then
	echo "Could not determine public IP of this host. Skipping SSL setup."
	exit 1
fi
echo "Host public IP: $PUBLIC_IP"

# Resolve domain to an IP
if command -v dig >/dev/null 2>&1; then
	DOMAIN_IP=$(dig +short "$DOMAIN" A 2>/dev/null | tail -1 || true)
elif command -v host >/dev/null 2>&1; then
	DOMAIN_IP=$(host -t A "$DOMAIN" 2>/dev/null | awk '/has address/{print $NF}' | head -1 || true)
elif command -v nslookup >/dev/null 2>&1; then
	DOMAIN_IP=$(nslookup "$DOMAIN" 2>/dev/null | awk '/^Address: /{print $2}' | head -1 || true)
elif command -v curl >/dev/null 2>&1; then
	DOMAIN_IP=$(curl -s --max-time 5 "https://dns.google/resolve?name=${DOMAIN}&type=A" 2>/dev/null \
		| grep -o '"data":"[0-9.]*"' | head -1 | cut -d'"' -f4 || true)
else
	echo "No DNS lookup tool (dig/host/nslookup/curl) found. Cannot verify domain DNS."
	exit 1
fi

if [ -z "$DOMAIN_IP" ]; then
	echo "Could not resolve IP for $DOMAIN. Ensure an A record points to this host."
	exit 1
fi
echo "Domain $DOMAIN resolves to: $DOMAIN_IP"

if [ "$DOMAIN_IP" != "$PUBLIC_IP" ]; then
	echo "DNS mismatch: $DOMAIN resolves to $DOMAIN_IP but this host's public IP is $PUBLIC_IP."
	echo "Point your domain's DNS A record to $PUBLIC_IP and re-run this script."
	exit 1
fi
echo "DNS verified: $DOMAIN points to this host."

# Install certbot with nginx plugin if not present
if ! command -v certbot >/dev/null 2>&1; then
	echo "certbot not found. Installing..."
	if command -v apt-get >/dev/null 2>&1; then
		sudo apt-get update
		sudo apt-get install -y certbot python3-certbot-nginx
	elif command -v dnf >/dev/null 2>&1; then
		sudo dnf install -y certbot python3-certbot-nginx
	elif command -v yum >/dev/null 2>&1; then
		sudo yum install -y certbot python3-certbot-nginx
	elif command -v pacman >/dev/null 2>&1; then
		sudo pacman -Sy --noconfirm certbot certbot-nginx
	elif command -v zypper >/dev/null 2>&1; then
		sudo zypper --non-interactive install certbot python3-certbot-nginx
	else
		echo "No supported package manager found. Install certbot manually."
		exit 1
	fi
	echo "certbot installed."
fi

# Ensure server_name in the nginx config contains the domain
if ! grep -Eq "^[[:space:]]*server_name[[:space:]].*${DOMAIN}" "$NGINX_DEFAULT_CONF"; then
	echo "Adding $DOMAIN to server_name in $NGINX_DEFAULT_CONF..."
	if grep -Eq "^[[:space:]]*server_name[[:space:]]" "$NGINX_DEFAULT_CONF"; then
		# Replace existing server_name value
		sudo sed -i "s/^\([[:space:]]*server_name\)[[:space:]][^;]*;/\1 $DOMAIN;/" "$NGINX_DEFAULT_CONF"
	else
		# Insert server_name inside the first server { block, after the opening brace
		sudo sed -i "0,/^[[:space:]]*server[[:space:]]*{/s//&\n    server_name $DOMAIN;/" "$NGINX_DEFAULT_CONF"
	fi
	sudo nginx -t
	if command -v systemctl >/dev/null 2>&1; then
		sudo systemctl reload nginx
	else
		sudo nginx -s reload
	fi
fi

# Run certbot to obtain certificate and configure SSL
echo "Running certbot for $DOMAIN..."
sudo certbot --nginx -d "$DOMAIN"

echo "SSL setup complete for $DOMAIN."
