.PHONY: fmt check build-uefi clean

fmt:
	cargo fmt --all

check:
	cargo check --target x86_64-unknown-uefi -p everest-boot

build-uefi:
	sh scripts/build-uefi.sh

clean:
	cargo clean
	rm -rf dist
