# Flute Listener

A real-time audio analysis TUI tool that visualizes frequency and time domain data in a terminal user interface.


![Screenshot of this TUI running](./assets/screenshot.png "Screenshot of this TUI running")

## Features

-   Real-time audio input and processing
-   Frequency domain visualization
-   Time domain visualization
-   Note detection

## Getting Started

### Prerequisites

-   Rust toolchain: [https://www.rust-lang.org/tools/install](https://www.rust-lang.org/tools/install)

### Installation

1.  Clone the repository:

    ```bash
    git clone <repository_url>
    cd flute-listener
    ```

2.  Build the project:

    ```bash
    cargo build --release
    ```

### Usage

Run the executable:

```bash
./target/release/flute-listener
```

### Controls

-   `q`: Quit the application
-   `d`: Switch to frequency chart screen
-   `m`: Switch to main screen

## Contributing

Contributions are welcome! Please feel free to submit a pull request.

## License

[MIT](./LICENSE)
