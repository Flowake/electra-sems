# Electra SEMS (Station Energy Management System)

A smart energy management system for electric vehicle charging stations that optimally allocates power across multiple chargers and manages battery storage.

## Quick Start

### Using Docker Compose

1. **Start the server:**
   ```bash
   docker-compose up --build
   ```

2. **Test the API:**
   ```bash
   curl http://localhost:3000/health
   curl http://localhost:3000/station/config
   ```

### Using Rust (For Development)

1. **Install Rust:** Visit [rustup.rs](https://rustup.rs/) and follow instructions

2. **Run the server:**
   ```bash
   cargo run -- --config examples/station_config.json --port 3000
   ```

3. **Run tests:**
   ```bash
   cargo test
   ```

## API Overview

### Common endpoints

- **GET** `/health` - Health check endpoint

### Station endpoints

- **GET** `/station/config` - Current station configuration
- **POST** `/station/config` - Change station configuration
- **GET** `/station/status` - Current active sessions

### Session endpoints

- **POST** `/sessions` - Start charging session
- **POST** `/sessions/{id}/power-update` - Update session power demand
- **POST** `/sessions/{id}/stop` - End charging session

## Configuration

The system loads station configuration from a JSON file:

```json
{
  "stationId": "ELECTRA_PARIS_15",
  "gridCapacity": 400,
  "chargers": [
    {
      "id": "CP001",
      "maxPower": 200,
      "connectors": 2
    }
  ],
  "battery": {
    "initialCapacity": 200,
    "power": 100
  }
}
```

Note: The battery handling has not been implemented yet, so this setting is ignored.

## Design Decisions

### Power Allocation Algorithm

The power allocation algorithm is designed to be fair and use the maximum available power while ensuring that the grid and chargers are not overloaded.

#### Assumptions

- A fair allocation means that every EV is offered the same amount of power,
but if it is not used to the fullest, then the remaining power is allocated to other EVs.
- If an EV do not use all its allocated power, then the power it uses is considered as its maximum power,
freeing power for other EVs.

#### Algorithm

The algorithm is implemented in the Rust file `crates/sems_core/src/allocator.rs`

The allocation is an iterative process that will look like this,
starting with all EVs having no allocated power:

1. Calculate the remaining power for the station.
2. Find EVs that are not at their maximum power and whose
charger is not at their maximum power either, and regroup them by charger.
3. Compute the fair share of the remaining power that each EV should receive. It effectively
redistributes the power that will not be used by EVs that have reached their maximum power.
4. For each charger:

- Compute the additional power to be allocated to its EVs such that the total power allocated
does not exceed the charger's capacity.
- Split the additional power (which might be lower than expected due to the chargers reaching maximum capacity)
among the EVs that are not at their maximum power.
- Allocate the power to the EVs while ensuring that the do not exceed their maximum power.

5. Repeat the steps 1-4 until one of the following conditions is met:
- All EVs have reached their maximum power. They would not accept more power.
- All chargers in use are at their maximum power. EVs might not be at their max power,
but it's not possible to allocate more to them without exceeding the charger's capacity.
- The grid's capacity is reached.

#### Example

A station with 330 kW capacity has 2 chargers with 200kW capacity, with the following EVs:

- Charger 1: 50kW EV and 150kW EV
- Charger 2: 150kW

1. Each EV should receive 110kW:

- Charger 1 - EV 1: 80kW/80kW (takes the maximum, leaving 30 kW)
- Charger 1 - EV 2: 110kW/150kW (takes all allocated power)
- Charger 2 - EV 1: 110kW/150kW (takes all allocated power)

2. 300 kW is being used, leaving 30kW to split. There are still 2 EVs with capacity left
and whose chargers are not at their maximum:

- Charger 1 - EV 1: 80kW/80kW (does not change)
- Charger 1 - EV 2: 120kW/150kW (should have received 15kW, but the charger has reached maximum capacity)
- Charger 2 - EV 1: 125kW/150kW (takes all allocated power)

3. 325 kW is being used. There are still 2 EVs with capacity left, but for one of them, the charger
is at capacity. Then 5 kW is split among one available EV:

- Charger 1 - EV 1: 80kW/80kW
- Charger 1 - EV 2: 120kW/150kW
- Charger 2 - EV 1: 130kW/150kW


### Architecture Choices

- **Rust**: Chosen for memory safety, performance, and excellent concurrency support
- **Axum**: Modern async web framework for high-performance APIs
- **Workspace Structure**: Separates core business logic (`sems_core`) from API layer (`sems_api`)
- **In-Memory State**: Simple and fast for a technical test. A database would be better suited for production.

## Testing

Run `cargo test` to execute the full test suite.
