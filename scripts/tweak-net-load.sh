#!/usr/bin/env bash
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo -e "\033[31mError: This script must be run as root (use sudo).\033[0m" >&2
    exit 1
fi

SYSCTL_FILE="/etc/sysctl.d/99-load-tweaks.conf"

# Kernel tuning for high connection load. BBR / qdisc / slow-start tweaks are
# handled separately in tweak-net.sh, so they are intentionally omitted here.
declare -A sysctl_settings=(
    # Requested core values.
    ["net.ipv4.tcp_max_syn_backlog"]="32768"
    ["net.core.somaxconn"]="32768"

    # Complementary high-load tuning.
    ["net.core.netdev_max_backlog"]="65536"
    ["net.ipv4.tcp_tw_reuse"]="1"
    ["net.ipv4.tcp_fin_timeout"]="15"
    ["net.ipv4.ip_local_port_range"]="1024 65535"
    ["net.ipv4.tcp_max_tw_buckets"]="1440000"
    ["net.ipv4.tcp_syncookies"]="1"
    ["fs.file-max"]="2097152"
    ["net.netfilter.nf_conntrack_max"]="1048576"
)

echo "Validating & applying high-load sysctl parameters..."

> "$SYSCTL_FILE"
applied=0

for key in "${!sysctl_settings[@]}"; do
    value="${sysctl_settings[$key]}"

    # Check if kernel recognizes this key.
    if sysctl -n "$key" &>/dev/null; then
        # Apply immediately to catch invalid values/rejections.
        if sysctl -w "$key=$value" &>/dev/null; then
            echo "$key = $value" >> "$SYSCTL_FILE"
            applied=$((applied + 1))
            echo -e "\033[32m$key = $value\033[0m"
        else
            echo -e "\033[33mKey '$key' exists, but kernel rejected value '$value'.\033[0m" >&2
        fi
    else
        echo -e "\033[33mSysctl key '$key' is not supported by this kernel.\033[0m" >&2
    fi
done

if (( applied > 0 )); then
    echo ""
    echo "Validating persistence..."
    if sysctl --system &>/dev/null; then
        echo -e "\033[32m$applied parameter(s) applied and persisted to $SYSCTL_FILE\033[0m"
    else
        echo -e "\033[33mPersistence validation failed. Check syntax in $SYSCTL_FILE\033[0m" >&2
        exit 1
    fi
else
    echo -e "\033[33mNo valid sysctl parameters were applied.\033[0m" >&2
    exit 1
fi

# --- nginx tuning ---

if ! command -v nginx >/dev/null 2>&1; then
    echo -e "\033[33mnginx is not installed. Skipping nginx tuning.\033[0m" >&2
    exit 0
fi

NGINX_CONF="/etc/nginx/nginx.conf"
if [[ ! -f "$NGINX_CONF" ]]; then
    echo -e "\033[31mCould not find $NGINX_CONF. Aborting nginx tuning.\033[0m" >&2
    exit 1
fi

WORKER_CONNECTIONS=8192
WORKER_RLIMIT_NOFILE=65536

echo ""
echo "Tuning nginx ($NGINX_CONF)..."

# Back up the original config once.
BACKUP="${NGINX_CONF}.bak.$(date +%Y%m%d%H%M%S)"
cp "$NGINX_CONF" "$BACKUP"
echo "Backed up existing config to $BACKUP"

# Set worker_rlimit_nofile at the main context.
if grep -Eq '^[[:space:]]*worker_rlimit_nofile[[:space:]]' "$NGINX_CONF"; then
    sed -i -E "s/^[[:space:]]*worker_rlimit_nofile[[:space:]]+[0-9]+;/worker_rlimit_nofile ${WORKER_RLIMIT_NOFILE};/" "$NGINX_CONF"
else
    # Insert after the worker_processes directive (or at top if absent).
    if grep -Eq '^[[:space:]]*worker_processes[[:space:]]' "$NGINX_CONF"; then
        sed -i -E "0,/^[[:space:]]*worker_processes[[:space:]].*$/s//&\nworker_rlimit_nofile ${WORKER_RLIMIT_NOFILE};/" "$NGINX_CONF"
    else
        sed -i "1i worker_rlimit_nofile ${WORKER_RLIMIT_NOFILE};" "$NGINX_CONF"
    fi
fi
echo -e "\033[32mworker_rlimit_nofile = ${WORKER_RLIMIT_NOFILE}\033[0m"

# Set worker_connections inside the events block.
if grep -Eq '^[[:space:]]*worker_connections[[:space:]]' "$NGINX_CONF"; then
    sed -i -E "s/^([[:space:]]*)worker_connections[[:space:]]+[0-9]+;/\1worker_connections ${WORKER_CONNECTIONS};/" "$NGINX_CONF"
else
    # Insert into the events block.
    sed -i -E "0,/^[[:space:]]*events[[:space:]]*\{/s//&\n    worker_connections ${WORKER_CONNECTIONS};/" "$NGINX_CONF"
fi
echo -e "\033[32mworker_connections = ${WORKER_CONNECTIONS}\033[0m"

# Raise the open files limit for the nginx service via a systemd drop-in.
if command -v systemctl >/dev/null 2>&1; then
    OVERRIDE_DIR="/etc/systemd/system/nginx.service.d"
    OVERRIDE_FILE="${OVERRIDE_DIR}/override.conf"
    mkdir -p "$OVERRIDE_DIR"
    cat > "$OVERRIDE_FILE" <<EOF
[Service]
LimitNOFILE=${WORKER_RLIMIT_NOFILE}
EOF
    echo -e "\033[32mLimitNOFILE = ${WORKER_RLIMIT_NOFILE} ($OVERRIDE_FILE)\033[0m"
fi

# Validate nginx configuration before reloading the service.
if ! nginx -t; then
    echo -e "\033[31mnginx config test failed. Restoring backup $BACKUP.\033[0m" >&2
    cp "$BACKUP" "$NGINX_CONF"
    exit 1
fi

if command -v systemctl >/dev/null 2>&1; then
    echo ""
    echo "Reloading systemd and restarting nginx..."
    systemctl daemon-reload
    systemctl restart nginx
    echo -e "\033[32mnginx restarted successfully.\033[0m"
else
    echo -e "\033[33msystemctl not found. Reload nginx manually to apply changes.\033[0m" >&2
fi
