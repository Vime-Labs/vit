#!/usr/bin/env bash
# install.sh — instala o compilador vit e a stdlib
#
# Uso (dentro do repo clonado):
#   bash install.sh
#
# O que faz:
#   1. Compila e instala o binário `vit` via cargo
#   2. Copia a stdlib para ~/.vit/lib/

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
RESET='\033[0m'

info()    { echo -e "${CYAN}[vit]${RESET} $*"; }
success() { echo -e "${GREEN}[vit]${RESET} $*"; }
error()   { echo -e "${RED}[vit]${RESET} $*" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VIT_LIB="$HOME/.vit/lib"

# ── 1. Dependências ───────────────────────────────────────────────────────────

check_dep() {
    command -v "$1" &>/dev/null || error "Dependência não encontrada: $1. Instale com: sudo apt install $2"
}

check_dep cargo  "cargo (via rustup: https://rustup.rs)"
check_dep clang  "clang"
check_dep llc    "llvm (ex: llvm-18)"

# ── 2. Compilar e instalar o binário ──────────────────────────────────────────

info "Compilando o compilador vit..."
RUSTFLAGS="-Awarnings" cargo install --path "$SCRIPT_DIR" --quiet
success "Binário 'vit' instalado em $(which vit)"

# ── 3. Instalar a stdlib ──────────────────────────────────────────────────────

LIB_SRC="$SCRIPT_DIR/lib"

[ -d "$LIB_SRC" ] || error "Diretório 'lib/' não encontrado em $SCRIPT_DIR"

info "Instalando stdlib em $VIT_LIB ..."
mkdir -p "$VIT_LIB"

cp "$LIB_SRC/"*.vit "$VIT_LIB/"
cp "$LIB_SRC/"*.c   "$VIT_LIB/"

success "Stdlib instalada:"
for f in "$VIT_LIB"/*.vit; do
    echo "  $(basename "$f")"
done

# ── 4. Verificar instalação ───────────────────────────────────────────────────

echo
info "Verificando instalação..."
vit --help > /dev/null 2>&1 && success "vit está funcionando." || error "Algo deu errado."

echo
success "Pronto! Experimente:"
echo "  vit run examples/hello.vit"
echo "  vit build meu_app.vit"
