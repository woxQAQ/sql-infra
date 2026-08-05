{ pkgs, ... }:

{
  packages = with pkgs; [
    git
    lld
    nodejs_22
    nushell
    pnpm
  ];

  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  # The Rust toolchain expects an external linker for WebAssembly. Keep the
  # linker in the development environment and configure Cargo explicitly.
  env.CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER = "${pkgs.lld}/bin/wasm-ld";

  scripts.playground.exec = "pnpm --dir playground dev";
  scripts.build-playground.exec = "pnpm --dir playground build";

  enterShell = # sh
    ''
      echo "SQL Infra development environment"
      echo "  Rust: $(rustc --version)"
      echo "  Node: $(node --version)"
      echo "  pnpm: $(pnpm --version)"

      # devenv uses Bash to initialize the environment. For an interactive
      # terminal, replace it with Nushell instead of starting a nested shell.
      if [ -t 0 ] && [ -t 1 ]; then
        echo "Entering Nushell ($(nu --version))"
        exec nu
      fi
    '';

  enterTest = ''
    cargo test --workspace
    pnpm --dir playground build
  '';
}
