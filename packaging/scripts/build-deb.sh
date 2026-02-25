#!/bin/bash
set -e

# Скрипт для создания .deb пакета
# Использование: ./build-deb.sh <version> <architecture>
# Например: ./build-deb.sh 0.1.0 amd64

VERSION=${1:-"0.1.0"}
ARCH=${2:-"amd64"}
PACKAGE_NAME="gofer"
MAINTAINER="gofer developers <dev@gofer.dev>"

echo "Building DEB package: ${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"

# Определяем target для Rust
if [ "$ARCH" = "amd64" ]; then
    RUST_TARGET="x86_64-unknown-linux-gnu"
elif [ "$ARCH" = "arm64" ]; then
    RUST_TARGET="aarch64-unknown-linux-gnu"
else
    echo "Unsupported architecture: $ARCH"
    exit 1
fi

# Собираем бинарник
echo "Building Rust binary for target: $RUST_TARGET"
cargo build --release --target $RUST_TARGET

# Создаем структуру пакета
PACKAGE_DIR="target/deb-package"
rm -rf $PACKAGE_DIR
mkdir -p $PACKAGE_DIR/usr/local/bin
mkdir -p $PACKAGE_DIR/lib/systemd/system
mkdir -p $PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME
mkdir -p $PACKAGE_DIR/DEBIAN

# Копируем бинарник
cp target/$RUST_TARGET/release/gofer $PACKAGE_DIR/usr/local/bin/
chmod 755 $PACKAGE_DIR/usr/local/bin/gofer

# Создаем systemd service файл
cat > $PACKAGE_DIR/lib/systemd/system/gofer.service << 'EOF'
[Unit]
Description=gofer MCP daemon
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/gofer daemon
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
EOF

# Копируем документацию
cp README.md $PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/ || true
cp LICENSE $PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/ || true

# Создаем control файл
cat > $PACKAGE_DIR/DEBIAN/control << EOF
Package: $PACKAGE_NAME
Version: $VERSION
Section: devel
Priority: optional
Architecture: $ARCH
Maintainer: $MAINTAINER
Description: Intelligent MCP server for Rust codebases
 gofer is a high-performance MCP (Model Context Protocol) server
 that provides intelligent indexing, semantic search, and token-efficient
 code reading capabilities. Features include:
 .
  * Semantic search with vector embeddings and re-ranking
  * AST parsing via tree-sitter (Rust, TypeScript, Python, Go, Vue)
  * Hybrid storage: SQLite (metadata) + LanceDB (vectors)
  * Incremental indexing with file watcher
  * Token-efficient tools (50-70% savings)
  * Built-in Prometheus metrics
Depends: libc6 (>= 2.31)
EOF

# Создаем postinst скрипт
cat > $PACKAGE_DIR/DEBIAN/postinst << 'EOF'
#!/bin/bash
set -e

# Перезагружаем systemd после установки
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

echo "gofer installed successfully!"
echo "To start the daemon: sudo systemctl start gofer"
echo "To enable on boot: sudo systemctl enable gofer"
echo ""
echo "For usage information, run: gofer --help"

exit 0
EOF

chmod 755 $PACKAGE_DIR/DEBIAN/postinst

# Создаем prerm скрипт
cat > $PACKAGE_DIR/DEBIAN/prerm << 'EOF'
#!/bin/bash
set -e

# Останавливаем сервис перед удалением
if [ -d /run/systemd/system ]; then
    systemctl stop gofer >/dev/null 2>&1 || true
    systemctl disable gofer >/dev/null 2>&1 || true
fi

exit 0
EOF

chmod 755 $PACKAGE_DIR/DEBIAN/prerm

# Создаем postrm скрипт
cat > $PACKAGE_DIR/DEBIAN/postrm << 'EOF'
#!/bin/bash
set -e

# Перезагружаем systemd после удаления
if [ "$1" = "purge" ] && [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

exit 0
EOF

chmod 755 $PACKAGE_DIR/DEBIAN/postrm

# Собираем пакет
echo "Building .deb package..."
dpkg-deb --build $PACKAGE_DIR target/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb

echo "✓ Package created: target/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
echo ""
echo "To install: sudo dpkg -i target/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
echo "To verify: dpkg -I target/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
