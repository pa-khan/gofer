#!/bin/bash
set -e

# Скрипт для создания .rpm пакета
# Использование: ./build-rpm.sh <version> <architecture>
# Например: ./build-rpm.sh 0.1.0 x86_64

VERSION=${1:-"0.1.0"}
ARCH=${2:-"x86_64"}
PACKAGE_NAME="gofer"

echo "Building RPM package: ${PACKAGE_NAME}-${VERSION}-1.${ARCH}.rpm"

# Проверяем наличие rpmbuild
if ! command -v rpmbuild &> /dev/null; then
    echo "Error: rpmbuild not found. Install with: sudo apt-get install rpm (Ubuntu/Debian) or sudo yum install rpm-build (RHEL/CentOS)"
    exit 1
fi

# Определяем target для Rust
if [ "$ARCH" = "x86_64" ]; then
    RUST_TARGET="x86_64-unknown-linux-gnu"
elif [ "$ARCH" = "aarch64" ]; then
    RUST_TARGET="aarch64-unknown-linux-gnu"
else
    echo "Unsupported architecture: $ARCH"
    exit 1
fi

# Собираем бинарник
echo "Building Rust binary for target: $RUST_TARGET"
cargo build --release --target $RUST_TARGET

# Создаем структуру для rpmbuild
RPMBUILD_DIR="$HOME/rpmbuild"
mkdir -p $RPMBUILD_DIR/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

# Создаем spec файл
SPEC_FILE="$RPMBUILD_DIR/SPECS/${PACKAGE_NAME}.spec"
cat > $SPEC_FILE << EOF
Name:           $PACKAGE_NAME
Version:        $VERSION
Release:        1%{?dist}
Summary:        Intelligent MCP server for Rust codebases
License:        MIT
URL:            https://github.com/pa-khan/gofer
BuildArch:      $ARCH

%description
gofer is a high-performance MCP (Model Context Protocol) server
that provides intelligent indexing, semantic search, and token-efficient
code reading capabilities. Features include:

* Semantic search with vector embeddings and re-ranking
* AST parsing via tree-sitter (Rust, TypeScript, Python, Go, Vue)
* Hybrid storage: SQLite (metadata) + LanceDB (vectors)
* Incremental indexing with file watcher
* Token-efficient tools (50-70% savings)
* Built-in Prometheus metrics

%prep
# No prep needed - binary already built

%build
# No build needed - binary already built

%install
# Создаем директории
mkdir -p %{buildroot}/usr/local/bin
mkdir -p %{buildroot}/lib/systemd/system
mkdir -p %{buildroot}/usr/share/doc/%{name}

# Копируем бинарник
cp $PWD/target/$RUST_TARGET/release/gofer %{buildroot}/usr/local/bin/
chmod 755 %{buildroot}/usr/local/bin/gofer

# Создаем systemd service
cat > %{buildroot}/lib/systemd/system/gofer.service << 'EOFSERVICE'
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
EOFSERVICE

# Копируем документацию
if [ -f $PWD/README.md ]; then
    cp $PWD/README.md %{buildroot}/usr/share/doc/%{name}/
fi
if [ -f $PWD/LICENSE ]; then
    cp $PWD/LICENSE %{buildroot}/usr/share/doc/%{name}/
fi

%files
/usr/local/bin/gofer
/lib/systemd/system/gofer.service
%doc /usr/share/doc/%{name}/*

%post
# Перезагружаем systemd после установки
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

echo "gofer installed successfully!"
echo "To start the daemon: sudo systemctl start gofer"
echo "To enable on boot: sudo systemctl enable gofer"
echo ""
echo "For usage information, run: gofer --help"

%preun
# Останавливаем сервис перед удалением
if [ \$1 -eq 0 ] && [ -d /run/systemd/system ]; then
    systemctl stop gofer >/dev/null 2>&1 || true
    systemctl disable gofer >/dev/null 2>&1 || true
fi

%postun
# Перезагружаем systemd после удаления
if [ \$1 -eq 0 ] && [ -d /run/systemd/system ]; then
    systemctl daemon-reload >/dev/null 2>&1 || true
fi

%changelog
* $(date "+%a %b %d %Y") gofer developers - $VERSION-1
- Release $VERSION
EOF

# Собираем RPM
echo "Building RPM package..."
cd $(pwd)
rpmbuild -bb $SPEC_FILE --define "_topdir $RPMBUILD_DIR" --define "_builddir $(pwd)"

# Копируем результат
OUTPUT_DIR="target"
mkdir -p $OUTPUT_DIR
cp $RPMBUILD_DIR/RPMS/$ARCH/${PACKAGE_NAME}-${VERSION}-1.*.rpm $OUTPUT_DIR/

echo "✓ Package created: $OUTPUT_DIR/${PACKAGE_NAME}-${VERSION}-1.$ARCH.rpm"
echo ""
echo "To install: sudo rpm -i $OUTPUT_DIR/${PACKAGE_NAME}-${VERSION}-1.$ARCH.rpm"
echo "To verify: rpm -qip $OUTPUT_DIR/${PACKAGE_NAME}-${VERSION}-1.$ARCH.rpm"
