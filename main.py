import sys
from pathlib import Path

# Add the root directory to sys.path so that 'src' and 'tools' are discoverable
root_dir = Path(__file__).parent.absolute()
if str(root_dir) not in sys.path:
    sys.path.insert(0, str(root_dir))

from src.SuomiCPU import main

if __name__ == "__main__":
    main()
