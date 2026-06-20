{
  description = "dumbsterfire - A simple IMAP email archiver";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }: let
    systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
    forAllSystems = nixpkgs.lib.genAttrs systems;
    nixpkgsFor = forAllSystems (system: import nixpkgs {
      system = system;
    });
    buildDeps = system: with nixpkgsFor.${system}; [
      openssl
      pkg-config
    ];
  in {
    packages = forAllSystems (system: {
      default = let
        pkgs = nixpkgsFor.${system};
        version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
        pname = (pkgs.lib.importTOML ./Cargo.toml).package.name;
        rustPlatform = pkgs.makeRustPlatform {
          rustc = pkgs.rustc;
          cargo = pkgs.cargo;
        };
      in pkgs.rustPlatform.buildRustPackage {
        inherit pname version;

        src = pkgs.lib.cleanSource ./.;
        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        buildInputs = buildDeps system;
        nativeBuildInputs = buildDeps system;
      };
    });

    devShells = forAllSystems (system: {
      default = let
        pkgs = nixpkgsFor.${system};
      in pkgs.mkShell {
        buildInputs = buildDeps system ++ [
          pkgs.rust-analyzer
          pkgs.cargo
          pkgs.rustc
          pkgs.rustfmt
          pkgs.rustPackages.clippy
        ];
      };
    });

    formatter = forAllSystems (system: nixpkgsFor.${system}.alejandra);
  };
}
