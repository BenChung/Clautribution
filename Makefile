DIST_DIR := dist/claudtributter
PACKAGE  := dist/claudtributter-plugin.zip

.PHONY: build test package clean

build:
	cargo build --release
	mkdir -p $(DIST_DIR)/bin $(DIST_DIR)/.claude-plugin $(DIST_DIR)/hooks
	cp target/release/claudtributter $(DIST_DIR)/bin/claudtributter
	cp plugin/plugin.json $(DIST_DIR)/.claude-plugin/plugin.json
	cp plugin/hooks.json $(DIST_DIR)/hooks/hooks.json

test:
	cargo test

package: build
	cd dist && zip -r claudtributter-plugin.zip claudtributter/

clean:
	cargo clean
	rm -rf dist
