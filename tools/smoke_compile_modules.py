from src.bios import discover_boot_modules
from src.program_loader import load_program_file


def main() -> None:
    modules = discover_boot_modules()
    compiled = 0
    for module in modules:
        if module.path is None:
            continue
        load_program_file(module.path, logger=None, memory_capacity=16 * 1024 * 1024)
        print(f"Compiled {module.name}: {module.path}")
        compiled += 1
    print(f"Compiled {compiled} BIOS modules")


if __name__ == "__main__":
    main()
