{
  description = "Simple REPL shell for untyped lambda expressions.";
  inputs = {
    nixpkgs.url = "nixpkgs";
    cf.url = "github:jzbor/cornflakes";
    cf.inputs.nixpkgs.follows = "nixpkgs";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { nixpkgs, cf, crane, ... }:
  cf.lib.flakeForDefaultSystems (system:
  with builtins;
  let
    pkgs = nixpkgs.legacyPackages.${system};
    craneLib = crane.mkLib pkgs;
    nativeBuildInputs = with pkgs; [
      clang
    ];
  in {
    ### PACKAGES ###
    packages = {
      default = craneLib.buildPackage {
        pname = "foliot";

        src = ./.;

        # Add extra inputs here or any other derivation settings
        # doCheck = true;
        inherit nativeBuildInputs;
      };
    };
  }) // {
    nixConfig = {
      extra-substituters = [ "https://cache.jzbor.de/public" ];
      extra-trusted-public-keys = [ "public:AdkE6qSLmWKFX4AptLFl+n+RTPIo1lrBhT2sPgfg5s4=" ];
    };
  };
}

