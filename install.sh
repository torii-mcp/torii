#!/bin/sh
# Instalador do Torii para Linux x86_64.
#
#   curl -fsSL https://raw.githubusercontent.com/torii-mcp/torii/main/install.sh | sh
#
# Nada aqui exige root: o binário vai para um diretório do usuário. O pacote só é
# extraído depois de o SHA-256 publicado bater, e o PATH só é alterado se você pedir.
#
# Opções (também aceitas como variáveis de ambiente):
#   --version vX.Y.Z   TORII_VERSION        versão a instalar (padrão: a última)
#   --dir <caminho>     TORII_INSTALL_DIR   destino (padrão: ~/.local/bin)
#   --add-to-path       TORII_ADD_TO_PATH=1 acrescenta o destino ao seu shell rc
#   --help

set -eu

REPO="torii-mcp/torii"
PLATFORM="linux-x86_64"
MARKER="# added by the Torii installer"

main() {
    parse_arguments "$@"
    require_platform
    require_tools

    version="${TORII_VERSION:-$(latest_version)}"
    case "$version" in
        v*) ;;
        *) version="v${version}" ;;
    esac
    install_dir="${TORII_INSTALL_DIR:-$HOME/.local/bin}"
    package="torii-${version}-${PLATFORM}"
    archive="${package}.tar.gz"
    base_url="https://github.com/${REPO}/releases/download/${version}"

    workdir="$(mktemp -d "${TMPDIR:-/tmp}/torii-install.XXXXXX")"
    # shellcheck disable=SC2064
    trap "rm -rf '$workdir'" EXIT INT TERM

    say "Downloading ${archive}"
    fetch "${base_url}/${archive}" "${workdir}/${archive}"
    fetch "${base_url}/${archive}.sha256" "${workdir}/${archive}.sha256"
    verify_checksum "$workdir" "$archive"

    tar -xzf "${workdir}/${archive}" -C "$workdir"
    binary="${workdir}/${package}/torii"
    [ -f "$binary" ] || fail "the archive did not contain the expected torii binary"

    previous="$(installed_version "${install_dir}/torii")"
    mkdir -p "$install_dir"
    # Substituição por rename: um torii em execução não é corrompido no meio.
    chmod 755 "$binary"
    mv -f "$binary" "${install_dir}/torii.new"
    mv -f "${install_dir}/torii.new" "${install_dir}/torii"

    installed="$("${install_dir}/torii" --version 2>/dev/null || echo unknown)"
    if [ -n "$previous" ] && [ "$previous" != "$installed" ]; then
        say "Updated ${previous} to ${installed} at ${install_dir}/torii"
    else
        say "Installed ${installed} at ${install_dir}/torii"
    fi

    handle_path "$install_dir"
    say "Next: torii init, then torii provider install <name>."
}

parse_arguments() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version)
                [ "$#" -ge 2 ] || fail "--version needs a value, for example --version v0.2.0"
                TORII_VERSION="$2"
                shift 2
                ;;
            --version=*)
                TORII_VERSION="${1#--version=}"
                shift
                ;;
            --dir)
                [ "$#" -ge 2 ] || fail "--dir needs a path"
                TORII_INSTALL_DIR="$2"
                shift 2
                ;;
            --dir=*)
                TORII_INSTALL_DIR="${1#--dir=}"
                shift
                ;;
            --add-to-path)
                TORII_ADD_TO_PATH=1
                shift
                ;;
            --help | -h)
                usage
                exit 0
                ;;
            *) fail "unknown option $1; run with --help" ;;
        esac
    done
}

usage() {
    sed -n '2,14p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//' || true
}

require_platform() {
    system="$(uname -s)"
    machine="$(uname -m)"
    [ "$system" = "Linux" ] ||
        fail "this installer only covers Linux; releases exist for Linux x86_64 and Windows x86_64, and other systems need \`cargo build --release\`"
    case "$machine" in
        x86_64 | amd64) ;;
        *) fail "no Torii release for ${machine}; build it from source with \`cargo build --release\`" ;;
    esac
}

require_tools() {
    have curl || have wget || fail "curl or wget is required"
    have tar || fail "tar is required"
    have sha256sum || have shasum || fail "sha256sum or shasum is required to verify the download"
}

have() {
    command -v "$1" >/dev/null 2>&1
}

# A tag da última release sai do redirecionamento de /releases/latest, que não
# consome cota da API; a API é o plano B.
latest_version() {
    url=""
    if have curl; then
        url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest" 2>/dev/null || true)"
    fi
    tag="${url##*/tag/}"
    if [ -n "$tag" ] && [ "$tag" != "$url" ]; then
        printf '%s\n' "$tag"
        return 0
    fi
    tag="$(download_stdout "https://api.github.com/repos/${REPO}/releases/latest" |
        sed -n 's/.*"tag_name" *: *"\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$tag" ] || fail "could not resolve the latest Torii version; pass --version vX.Y.Z"
    printf '%s\n' "$tag"
}

fetch() {
    if have curl; then
        curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1" ||
            fail "failed to download $1"
        return 0
    fi
    wget -qO "$2" "$1" || fail "failed to download $1"
}

download_stdout() {
    if have curl; then
        curl -fsSL --proto '=https' --tlsv1.2 "$1" || true
        return 0
    fi
    wget -qO- "$1" || true
}

verify_checksum() {
    dir="$1"
    file="$2"
    expected="$(awk '{print $1; exit}' "${dir}/${file}.sha256" | tr -d '\r')"
    [ -n "$expected" ] || fail "the published checksum file was empty"
    if have sha256sum; then
        actual="$(sha256sum "${dir}/${file}" | awk '{print $1}')"
    else
        actual="$(shasum -a 256 "${dir}/${file}" | awk '{print $1}')"
    fi
    [ "$expected" = "$actual" ] ||
        fail "checksum mismatch for ${file}: expected ${expected}, got ${actual}"
    say "Checksum verified"
}

installed_version() {
    [ -x "$1" ] || return 0
    "$1" --version 2>/dev/null || true
}

handle_path() {
    dir="$1"
    case ":${PATH}:" in
        *":${dir}:"*)
            return 0
            ;;
    esac
    if [ "${TORII_ADD_TO_PATH:-0}" != "1" ]; then
        say "${dir} is not in your PATH. Add it with:"
        printf '\n    export PATH="%s:$PATH"\n\n' "$dir" >&2
        say "Or rerun this installer with --add-to-path."
        return 0
    fi
    rc="$(shell_rc)"
    if [ -f "$rc" ] && grep -Fq "$MARKER" "$rc"; then
        say "${rc} already carries the Torii PATH line"
        return 0
    fi
    mkdir -p "$(dirname "$rc")"
    case "$rc" in
        */config.fish)
            printf '\n%s\nfish_add_path %s\n' "$MARKER" "$dir" >>"$rc"
            ;;
        *)
            printf '\n%s\nexport PATH="%s:$PATH"\n' "$MARKER" "$dir" >>"$rc"
            ;;
    esac
    say "Added ${dir} to PATH in ${rc}; open a new shell to pick it up"
}

shell_rc() {
    case "${SHELL:-}" in
        */zsh) printf '%s\n' "${ZDOTDIR:-$HOME}/.zshrc" ;;
        */bash) printf '%s\n' "$HOME/.bashrc" ;;
        */fish) printf '%s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish" ;;
        *) printf '%s\n' "$HOME/.profile" ;;
    esac
}

say() {
    printf 'torii: %s\n' "$1" >&2
}

fail() {
    printf 'torii: %s\n' "$1" >&2
    exit 1
}

main "$@"
