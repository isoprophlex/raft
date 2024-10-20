#!/bin/bash

# Directory containing init files
ROOT_DIR=$(pwd)
INIT_DIR="$ROOT_DIR/init_history"

# Check if directory exists
if [ ! -d "$INIT_DIR" ]; then
  echo "Directory $INIT_DIR does not exist."
  exit 1
fi

# Clean function
clean() {
  if [[ "$1" == "" ]]; then
    echo "Please provide a nodename to clean or 'all' to clean all."
    exit 1
  fi
  if [[ "$1" == "all" ]]; then
    # Delete all files in the directory
    rm -f "$INIT_DIR"/*
    echo "All files in $INIT_DIR have been deleted."
  else
    # Delete files matching the specified nodename
    rm -f "$INIT_DIR/init_$1.txt"
    echo "Running History for $1 has been cleared."
  fi
}

# Call clean with the argument passed
clean "$1"
