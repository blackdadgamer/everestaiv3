.PHONY: fmt check build-uefi test-qemu clean

fmt:
	cargo fmt --all

check:
	cargo check --target x86_64-unknown-uefi -p everest-boot

build-uefi:
	sh scripts/build-uefi.sh

test-qemu:
	sh scripts/test-qemu.sh

clean:
	cargo clean
	rm -rf dist
