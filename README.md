# chatui

A high-performance, lightweight desktop AI wrapper for LLM and VLM inference, built with Tauri, Leptos (Rust/Wasm CSR), and Tailwind CSS.

## 🛠 Prerequisites

Ensure you have the following installed on your system:

1. **Rust & Cargo**: Follow instructions at [rustup.rs](https://rustup.rs/).
2. **WebAssembly Target**: Add the target required for compiled frontend compilation:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```
3. **Trunk**: The WebAssembly bundler for Rust. Install it via cargo:
   ```bash
   cargo install --locked trunk
   ```
4. **Node.js & npm**: Required to build the Tailwind CSS styles. Install it from [nodejs.org](https://nodejs.org/).
5. **Tauri CLI**: Install the Tauri v2 command-line interface globally:
   ```bash
   cargo install tauri-cli --version "^2.0.0"
   ```

## 🚀 Running in Development

### 1. Install Node Dependencies
Install Tailwind CSS tooling dependencies:
```bash
npm install
```

### 2. Compile Tailwind Styles
Generate the CSS styling sheet used by the frontend:
```bash
npm run build:css
```
> **Tip:** You can run `npm run watch:css` in a separate terminal to automatically rebuild the stylesheet as you make UI styling updates.

### 3. Start Development Server
Launch the Tauri desktop window in development mode. This will automatically spin up `trunk serve` for the WebAssembly frontend and hot-reload the application on changes:
```bash
cargo tauri dev
```

## 📦 Building for Production

To package and compile the production-ready installer and binaries:
```bash
# Ensure CSS is built
npm run build:css

# Build the desktop bundles
cargo tauri build
```

The resulting installers (dmg/pkg/msi/etc.) will be located in the `src-tauri/target/release/bundle/` directory.

## 🧪 Testing

To run the Rust tests:
```bash
# Test the shared crate models
cargo test -p shared

# Test the backend Tauri shell
cargo test -p chatui
```
