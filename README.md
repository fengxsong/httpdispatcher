# HTTP Dispatcher

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

HTTP Dispatcher is a high-performance HTTP request processing and forwarding server written in Rust. It supports data transformation using VRL (Vector Remap Language) and provides flexible configuration options for processing and forwarding HTTP requests.

## Features

- 🚀 High-performance HTTP request processing
- 🔄 Data transformation using VRL (Vector Remap Language)
- ⚙️ Flexible YAML-based configuration
- 🔌 Multiple target forwarding support
- 🔄 Built-in retry mechanism with configurable attempts
- 🔍 Built-in `/echo` endpoint for debugging
- 📝 Request templating with variable substitution
- 📊 Detailed logging with structured output
- 🔑 Custom HTTP headers and query parameters support
- 🛡️ Configurable request size limits
- ⚡ Async processing with Tokio runtime

## System Requirements

- Rust 1.70 or higher
- Supported operating systems:
  - Linux
  - macOS
  - Windows

## Quick Start

1. Clone the repository:
```bash
git clone https://github.com/fengxsong/httpdispatcher.git
cd httpdispatcher
```

2. Build the project:
```bash
cargo build --release
```

3. Run the service:
```bash
cargo run --release
```

## Configuration

The configuration file uses YAML format with the default filename `config.yaml`. The configuration file contains the following main sections:

### Input Source Configuration
```yaml
sources:
  http_input:
    type: http
    address: 0.0.0.0
    port: 9090
    path: /ingest
    max_body_size_bytes: 1048576  # 1MB
```

### Transform Configuration
```yaml
transforms:
  log_processor:
    type: remap
    inputs: ["http_input"]
    source: |
      .metadata = {
        "processed_at": now(),
        "event_source": "alertmanager",
        "event_type": .value,
        "priority": "high"
      }
      .
```

### Output Configuration
```yaml
sinks:
  api_output:
    type: http
    inputs: ["log_processor"]
    uri: "https://api.example.com/{{ endpoint }}"
    method: POST
    encoding: json
    template: |
      {
        "event_id": "{{ id }}",
        "payload": {{ to_json(data) }},
        "timestamp": "{{ metadata.timestamp }}"
      }
    headers:
      Authorization: "Bearer {{ metadata.api_key }}"
      Content-Type: "application/json"
    timeout_ms: 5000
    retry_attempts: 3
    retry_interval_ms: 1000
```

## Usage Examples

### Basic Request
```bash
curl -X POST http://localhost:9090/ingest \
  -H "Content-Type: application/json" \
  -d '{"value": "test_event", "data": {"key": "value"}}'
```

### Debug Endpoint
```bash
curl -X POST http://localhost:9090/echo \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello, World!"}'
```

## Development

### Dependencies

Main dependencies include:
- tokio: Async runtime
- axum: Web framework
- vrl: Data transformation language
- serde: Serialization/deserialization
- reqwest: HTTP client
- tracing: Logging
- tower: Middleware framework
- tower-http: HTTP-specific middleware
- handlebars: Template engine

### Building and Testing

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run

# Build for production
cargo build --release
```

## Project Structure

```
httpdispatcher/
├── src/
│   ├── main.rs           # Application entry point
│   ├── config/           # Configuration handling
│   ├── server/           # HTTP server implementation
│   ├── transform/        # Data transformation logic
│   └── sink/            # Output handling
├── config.yaml          # Default configuration
├── Cargo.toml          # Project dependencies
└── README.md           # This file
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Author

Generated with Cursor

## Acknowledgments

- [Vector](https://vector.dev/) for the VRL language inspiration
- [Axum](https://github.com/tokio-rs/axum) for the excellent web framework
- [Tokio](https://tokio.rs/) for the async runtime