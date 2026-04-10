#!/usr/bin/env bash
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo -e "\033[31mError: This script must be run as root (use sudo).\033[0m" >&2
    exit 1
fi

CONFIG_FILE="/etc/sysctl.d/99-custom-tweaks.conf"

declare -A sysctl_settings=(
    ["net.ipv4.tcp_slow_start_after_idle"]="0"
    ["net.core.default_qdisc"]="fq"
    ["net.ipv4.tcp_congestion_control"]="bbr"
)

echo "Validating & applying network sysctl parameters..."

# Safely load BBR module (harmless if already built-in/loaded)
if ! modprobe tcp_bbr 2>/dev/null; then
    echo -e "\033[33mtcp_bbr module not available. Kernel may lack BBR support.\033[0m" >&2
fi

# Verify BBR is actually recognized by the kernel
AVAILABLE_CC=$(sysctl -n net.ipv4.tcp_available_congestion_control 2>/dev/null || true)
if [[ ! "$AVAILABLE_CC" =~ bbr ]]; then
    echo -e "\033[31mbbr is not in available congestion controls. Aborting.\033[0m" >&2
    echo -e "\033[31mAvailable: $AVAILABLE_CC\033[0m" >&2
    exit 1
fi
echo -e "\033[32mBBR congestion control is available.\033[0m"

# Apply immediately & build persistence file
> "$CONFIG_FILE"
applied=0

for key in "${!sysctl_settings[@]}"; do
    value="${sysctl_settings[$key]}"

    # Check if kernel recognizes this key
    if sysctl -n "$key" &>/dev/null; then
        # Apply immediately to catch invalid values/rejections
        if sysctl -w "$key=$value" &>/dev/null; then
            echo "$key = $value" >> "$CONFIG_FILE"
            applied=$((applied + 1))
            echo -e "\033[32m$key = $value\033[0m"
        else
            echo -e "\033[33mKey '$key' exists, but kernel rejected value '$value'.\033[0m" >&2
        fi
    else
        echo -e "\033[33mSysctl key '$key' is not supported by this kernel.\033[0m" >&2
    fi
done

# Validate persistence (mimics boot behavior)
if (( applied > 0 )); then
    echo ""
    echo "Validating persistence..."
    if sysctl --system &>/dev/null; then
        echo -e "\033[32m$applied parameter(s) applied and persisted to $CONFIG_FILE\033[0m"
        echo -e "\033[32mActive congestion control: $(sysctl -n net.ipv4.tcp_congestion_control)\033[0m"
    else
        echo -e "\033[33mPersistence validation failed. Check syntax in $CONFIG_FILE\033[0m" >&2
        exit 1
    fi
else
    echo -e "\033[33mNo valid parameters were applied.\033[0m" >&2
    exit 1
fi