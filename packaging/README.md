# Packaging

Эта директория содержит скрипты и конфигурацию для создания пакетов gofer для различных дистрибутивов Linux.

## Доступные форматы пакетов

- **.deb** - для Debian/Ubuntu и производных
- **.rpm** - для RHEL/CentOS/Fedora и производных

## Архитектуры

- **amd64/x86_64** - Intel/AMD 64-bit
- **arm64/aarch64** - ARM 64-bit (для серверов и Raspberry Pi 4+)

## Использование скриптов

### Сборка всех пакетов

```bash
cd packaging/scripts
./build-all.sh 0.1.0
```

### Сборка только .deb пакетов

```bash
cd packaging/scripts
./build-deb.sh 0.1.0 amd64   # для x86_64
./build-deb.sh 0.1.0 arm64   # для ARM64
```

### Сборка только .rpm пакетов

```bash
cd packaging/scripts
./build-rpm.sh 0.1.0 x86_64    # для x86_64
./build-rpm.sh 0.1.0 aarch64   # для ARM64
```

## Требования

### Для .deb пакетов

- `dpkg-deb` (обычно уже установлен на Debian/Ubuntu)
- Rust toolchain с поддержкой нужных targets

### Для .rpm пакетов

Ubuntu/Debian:
```bash
sudo apt-get install rpm
```

RHEL/CentOS/Fedora:
```bash
sudo yum install rpm-build
```

### Cross-compilation для ARM64

Ubuntu/Debian:
```bash
# Для ARM64
sudo apt-get install gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu
```

## Установка пакетов

### Debian/Ubuntu (.deb)

```bash
# Установка
sudo dpkg -i gofer_0.1.0_amd64.deb

# Проверка установленного пакета
dpkg -l | grep gofer

# Удаление
sudo dpkg -r gofer
```

### RHEL/CentOS/Fedora (.rpm)

```bash
# Установка
sudo rpm -i gofer-0.1.0-1.x86_64.rpm

# Проверка установленного пакета
rpm -qa | grep gofer

# Удаление
sudo rpm -e gofer
```

## Управление сервисом

После установки пакета:

```bash
# Запуск daemon
sudo systemctl start gofer

# Остановка daemon
sudo systemctl stop gofer

# Перезапуск daemon
sudo systemctl restart gofer

# Автозапуск при загрузке системы
sudo systemctl enable gofer

# Отключить автозапуск
sudo systemctl disable gofer

# Просмотр статуса
sudo systemctl status gofer

# Просмотр логов
sudo journalctl -u gofer -f
```

## Структура пакетов

Все пакеты устанавливают:

- `/usr/local/bin/gofer` - основной исполняемый файл
- `/lib/systemd/system/gofer.service` - systemd service файл
- `/usr/share/doc/gofer/` - документация (README, LICENSE)

## Автоматическая сборка в CI/CD

Пакеты автоматически собираются через GitHub Actions при создании release:

1. Создайте тег: `git tag v0.1.0 && git push origin v0.1.0`
2. GitHub Actions соберет все пакеты
3. Пакеты будут доступны на странице релиза

Alternatively, запустите workflow вручную через GitHub UI.

## Проверка качества пакета

### DEB пакеты

```bash
# Информация о пакете
dpkg -I gofer_0.1.0_amd64.deb

# Содержимое пакета
dpkg -c gofer_0.1.0_amd64.deb

# Lintian проверка (опционально)
lintian gofer_0.1.0_amd64.deb
```

### RPM пакеты

```bash
# Информация о пакете
rpm -qip gofer-0.1.0-1.x86_64.rpm

# Содержимое пакета
rpm -qlp gofer-0.1.0-1.x86_64.rpm

# Rpmlint проверка (опционально)
rpmlint gofer-0.1.0-1.x86_64.rpm
```

## Известные ограничения

- Пакеты предполагают наличие glibc 2.31+ (Ubuntu 20.04+, Debian 11+)
- Для более старых систем рассмотрите использование musl-based сборок
- ARM64 пакеты требуют cross-compilation на x86_64 хостах
