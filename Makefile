all: build install

build:
	cargo build

install:
	cargo install --path .