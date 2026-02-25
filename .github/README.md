# CI/CD Setup

Полная инфраструктура для непрерывной интеграции и автоматической сборки пакетов gofer.

## Workflows

### 1. CI (Continuous Integration)
**Файл**: `.github/workflows/ci.yml`

Запускается на каждый push/PR в ветки `main` и `develop`.

#### Jobs:

**Test Suite**
- Матричное тестирование: `ubuntu-latest`, `ubuntu-22.04`
- Только stable Rust
- `fail-fast: false` - все тесты выполняются даже при ошибках
- Кэширование cargo registry, index, target
- Шаги:
  - `cargo check --all-features`
  - `cargo test --all-features --no-fail-fast`

**Linting**
- Форматирование: `cargo fmt --all -- --check` (строгое)
- Статический анализ: `cargo clippy --all-features` (предупреждения не блокируют)
- Установка компонентов через `rustup component add`

**Security Audit**
- `cargo audit` для проверки уязвимостей
- `continue-on-error: true` - не блокирует CI
- Игнорирует placeholder advisory

#### Особенности:
- `RUSTC_WRAPPER: ""` - отключает sccache (избегает проблем в CI)
- Автоматическая установка rustfmt/clippy компонентов
- Полное кэширование для ускорения сборок

### 2. Release (Continuous Delivery)
**Файл**: `.github/workflows/release.yml`

Запускается при:
- Push тега `v*.*.*`
- Ручном запуске через workflow_dispatch

#### Платформы и форматы:

**Linux binaries** (tarball):
- `x86_64-unknown-linux-gnu` (стандартная glibc)
- `x86_64-unknown-linux-musl` (статическая сборка)
- `aarch64-unknown-linux-gnu` (ARM64)

**Distribution packages**:
- **.deb** для Debian/Ubuntu (amd64, arm64)
- **.rpm** для RHEL/CentOS/Fedora (x86_64, aarch64)

#### Jobs:

1. **create-release**: Создаёт GitHub Release
2. **build-linux**: Собирает бинарники для всех платформ
3. **build-packages**: Создаёт .deb и .rpm пакеты

Все артефакты автоматически загружаются в Release.

## Использование

### Автоматический релиз:
```bash
git tag v0.1.0
git push origin v0.1.0
```

### Ручной релиз:
1. Перейти в Actions → Release
2. Нажать "Run workflow"
3. Указать версию (например, `v0.1.0`)

### Локальная проверка CI:
```bash
# Проверки перед push
cargo fmt --all -- --check
cargo check --all-features
cargo clippy --all-features
cargo test --all-features

# Или всё сразу
./packaging/scripts/ci-check.sh  # (если создать такой скрипт)
```

## Troubleshooting

### Проблема: sccache not found
**Решение**: Добавлен `RUSTC_WRAPPER: ""` в env

### Проблема: rustfmt/clippy not installed
**Решение**: Добавлен шаг `rustup component add rustfmt clippy`

### Проблема: Тесты падают
1. Проверить локально: `cargo test --all-features`
2. Проверить логи в GitHub Actions
3. Убедиться, что все зависимости доступны

### Проблема: Release build fails
1. Проверить cross-compilation tools установлены
2. Для ARM64 нужен `gcc-aarch64-linux-gnu`
3. Для musl нужен `musl-tools`

## Кэширование

CI использует кэши для ускорения:
- `~/.cargo/registry` - скачанные crates
- `~/.cargo/git` - git зависимости
- `target/` - артефакты сборки

Ключ кэша включает:
- ОС runner
- Версия Rust (для test matrix)
- Hash `Cargo.lock`

## Метрики производительности

Примерное время выполнения (без кэша):
- Test Suite: 15-20 минут
- Linting: 5-10 минут
- Security Audit: 2-5 минут
- Release build (all platforms): 30-40 минут

С кэшем:
- Test Suite: 3-5 минут
- Linting: 1-2 минуты
- Security Audit: 1 минута
- Release build: 10-15 минут

## Будущие улучшения

- [ ] Добавить coverage отчёты (tarpaulin)
- [ ] Docker image сборка
- [ ] Публикация в crates.io
- [ ] Бенчмарки в CI
- [ ] Проверка документации
- [ ] Release notes автогенерация
