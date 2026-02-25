#!/bin/bash
set -e

# Главный скрипт для сборки всех пакетов
# Использование: ./build-all.sh <version>
# Например: ./build-all.sh 0.1.0

VERSION=${1:-"0.1.0"}

echo "=========================================="
echo "Building all packages for version $VERSION"
echo "=========================================="
echo ""

# Получаем директорию скрипта
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# DEB пакеты
echo "Building DEB packages..."
bash "$SCRIPT_DIR/build-deb.sh" $VERSION amd64
bash "$SCRIPT_DIR/build-deb.sh" $VERSION arm64

echo ""

# RPM пакеты
echo "Building RPM packages..."
bash "$SCRIPT_DIR/build-rpm.sh" $VERSION x86_64
bash "$SCRIPT_DIR/build-rpm.sh" $VERSION aarch64

echo ""
echo "=========================================="
echo "✓ All packages built successfully!"
echo "=========================================="
echo ""
echo "Created packages:"
ls -lh target/*.deb target/*.rpm 2>/dev/null || echo "No packages found in target/"
