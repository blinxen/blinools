# Fedora sources /etc/profile.d scripts during shell initialization.
load_api_keys() {
    [[ -d "${HOME}/.api-keys" ]] || return 0

    local key_script
    for key_script in "${HOME}/.api-keys/"*.sh; do
        if [[ -f "$key_script" && -r "$key_script" ]]; then
            . "$key_script"
        fi
    done
}

load_api_keys
unset -f load_api_keys
