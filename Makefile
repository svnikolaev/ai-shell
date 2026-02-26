# Makefile для ai-shell

# Имя символической ссылки (по умолчанию ai)
LINK_NAME ?= ai

.PHONY: all test build install link clean uninstall

# Цель по умолчанию: тест, сборка, установка и создание ссылки
all: test build install link

# Запуск тестов
test:
	cargo test

# Сборка релизной версии
build:
	cargo build --release

# Установка через cargo install из текущего пути
install:
	cargo install --path .

# Создание символической ссылки с именем $(LINK_NAME) в ~/.cargo/bin
link:
	@if [ -f ~/.cargo/bin/ai-shell ]; then \
		ln -sf ~/.cargo/bin/ai-shell ~/.cargo/bin/$(LINK_NAME); \
		echo "Ссылка создана: ~/.cargo/bin/$(LINK_NAME) -> ~/.cargo/bin/ai-shell"; \
	else \
		echo "Бинарник ai-shell не найден в ~/.cargo/bin. Сначала выполните make install."; \
		exit 1; \
	fi

# Очистка временных файлов сборки
clean:
	cargo clean

# Удаление установленного бинарника и ссылки
uninstall:
	@rm -f ~/.cargo/bin/ai-shell ~/.cargo/bin/$(LINK_NAME)
	@echo "Удалены ai-shell и $(LINK_NAME) из ~/.cargo/bin (если существовали)."
