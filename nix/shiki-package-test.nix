# Scratch derivation used only to validate the future nixpkgs package.nix
# draft against this repo's own checkout, in CI (see
# .github/workflows/nix-package-check.yml). Not the final nixpkgs submission
# file — that one will fetch the tagged release via fetchFromGitHub instead
# of using `src = ./..`, and won't live in this repo at all.
{ pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/archive/nixos-unstable.tar.gz") { } }:

pkgs.rustPlatform.buildRustPackage {
  pname = "shiki";
  version = "0.9.5";

  src = ./..;

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = with pkgs; [
    pkg-config
    cmake
    perl
  ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isx86_64 [ pkgs.nasm ];

  buildInputs = with pkgs; [
    openssl
    oniguruma
  ];

  env = {
    # git2's Cargo.toml hardcodes the "vendored-openssl" feature (needed for
    # the project's own cross-platform release binaries); this overrides
    # openssl-sys's build.rs to link nixpkgs' own openssl instead of
    # compiling OpenSSL from source, same trick nixpkgs' gitui package uses
    # for the identical git2 vendored-openssl situation.
    OPENSSL_NO_VENDOR = 1;
    # syntect's default features pull in onig_sys (oniguruma C library) for
    # regex-onig; this links nixpkgs' oniguruma via pkg-config instead of
    # compiling its own bundled copy, same as nixpkgs' atac package.
    RUSTONIG_SYSTEM_LIBONIG = true;
  };

  meta = {
    description = "TUI note-taking app with a Yazi-style three-pane layout";
    homepage = "https://github.com/sazardev/shiki";
    license = pkgs.lib.licenses.mit;
    mainProgram = "shiki";
  };
}
