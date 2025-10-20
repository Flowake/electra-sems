#!/usr/bin/env python3
"""
Power Sharing Scenario Test Script

Usage:
    python3 scripts/test_power_sharing.py [--url http://localhost:3000]

Requirements:
    pip install requests

Make sure your SEMS server is running with the test configuration before running this script.
"""

import argparse
import time
from typing import Any

try:
    import requests
except ImportError:
    print("Please install the 'requests' library: pip install requests")
    exit(1)


class SEMSClient:
    def __init__(self, base_url: str = "http://localhost:3000"):
        self.base_url = base_url.rstrip("/")
        self.session = requests.Session()

    def health_check(self) -> bool:
        """Check if the server is healthy"""
        try:
            response = self.session.get(f"{self.base_url}/health", timeout=5)
            return response.status_code == 200
        except:
            return False

    def get_station_config(self) -> dict[str, Any]:
        """Get the station configuration"""
        response = self.session.get(f"{self.base_url}/station/config")
        response.raise_for_status()
        return response.json()

    def set_station_config(self, config: dict[str, Any]) -> dict[str, Any]:
        """Set the station configuration"""
        response = self.session.post(f"{self.base_url}/station/config", json=config)
        response.raise_for_status()
        return response.json()

    def get_station_status(self) -> dict[str, Any]:
        """Get the current station status with all sessions"""
        response = self.session.get(f"{self.base_url}/station/status")
        response.raise_for_status()
        return response.json()

    def print_station_status(self) -> None:
        """Print the current station status with all sessions"""
        sessions = self.get_station_status().get("sessions", {})
        total_allocated_power = sum(
            session["allocatedPower"] for session in sessions.values()
        )
        print(f"   Total Allocated Power: {total_allocated_power}kW")
        for session in sessions.values():
            print_session_info(session)

    def create_session(
        self, charger_id: str, connector_idx: int, vehicle_max_power: int
    ) -> dict[str, Any]:
        """Create a new charging session"""
        payload = {
            "connectorId": {"chargerId": charger_id, "idx": connector_idx},
            "vehicleMaxPower": vehicle_max_power,
        }
        response = self.session.post(f"{self.base_url}/sessions", json=payload)
        if not response.ok:
            error_msg = response.text
            try:
                error_json = response.json()
                error_msg = error_json.get("error", error_msg)
            except:
                pass
            raise Exception(
                f"Failed to create session: {response.status_code} - {error_msg}"
            )
        return response.json()

    def stop_session(self, session_id: str) -> None:
        """Stop a charging session"""
        response = self.session.post(f"{self.base_url}/sessions/{session_id}/stop")
        response.raise_for_status()

    def power_update(self, session_id: str, consumed_power: int) -> dict[str, Any]:
        """Update the power consumption for a session"""
        payload = {"consumedPower": consumed_power}
        response = self.session.post(
            f"{self.base_url}/sessions/{session_id}/power-update", json=payload
        )
        response.raise_for_status()
        return response.json()


def print_banner(title: str):
    """Print a formatted banner"""
    print(f"\n{'=' * 60}")
    print(f"  {title}")
    print(f"{'=' * 60}")


def print_section(title: str):
    """Print a section header"""
    print(f"\n{'-' * 40}")
    print(f"🔍 {title}")
    print(f"{'-' * 40}")


def print_session_info(session: dict[str, Any], label: str = "Session"):
    """Print formatted session information"""
    connector = session["connectorId"]
    print(f"   {label}: {session['sessionId'][:8]}...")
    print(f"      Connector: {connector['chargerId']}:{connector['idx']}")
    print(f"      Max Power: {session['vehicleMaxPower']}kW")
    print(f"      Allocated: {session['allocatedPower']}kW")


def scenario_1(client: SEMSClient):
    """Validation of Power Sharing Scenario 1

    - Station configuration: {"gridCapacity": 400, "chargers": [{"id": "CP001", "maxPower": 200, "connectors": 2}]}
    - T0: 2 vehicles start charging, each accepting 150kW max
    - Expected: Each gets eventually ~100kW (200/2)
    """
    # Get and validate station configuration
    print_section("Station Configuration")
    _ = client.set_station_config(
        {
            "stationId": "TEST_STATION_POWER_SHARING",
            "gridCapacity": 400,
            "chargers": [{"id": "CP001", "maxPower": 200, "connectors": 2}],
        }
    )
    config = client.get_station_config()
    print(f"   Station Config: {config}")

    charger = config["chargers"][0]  # Use first charger

    # Test scenario setup
    charger_id = charger["id"]
    charger_max_power = charger["maxPower"]
    expected_power_per_session = charger_max_power // 2  # Expect fair sharing

    print_section("T0: Starting Two Vehicles")
    print(f"Expected power per session: ~{expected_power_per_session}kW")

    # Start first vehicle
    print("\n🔌 Starting Vehicle 1 on CP001:1 (150kW max)")
    session1_response = client.create_session(charger_id, 1, 150)
    session1 = session1_response["session"]
    print_session_info(session1, "Vehicle 1")

    # Small delay to ensure allocation is stable
    time.sleep(0.5)

    # Start second vehicle
    print("\n🔌 Starting Vehicle 2 on CP001:2 (150kW max)")
    session2_response = client.create_session(charger_id, 2, 150)
    session2 = session2_response["session"]
    print_session_info(session2, "Vehicle 2")

    print("\n🔄 Triggering power reallocation for vehicles...")
    for session, name in [
        (session1, "Vehicle 1"),
        (session2, "Vehicle 2"),
    ]:
        updated = client.power_update(session["sessionId"], session["allocatedPower"])
        session["allocatedPower"] = updated["session"]["allocatedPower"]
        session["vehicleMaxPower"] = updated["session"]["vehicleMaxPower"]
        print(f"   {name} reallocated to: {updated['session']['allocatedPower']}kW")

    print_section("Final Session Status")
    client.print_station_status()


def scenario_2(client: SEMSClient):
    """Validation of Power Sharing Scenario 2

    Station configuration: {"gridCapacity": 400, "chargers": [
        {"id": "CP001", "maxPower": 300, "connectors": 2},
        {"id": "CP002", "maxPower": 300, "connectors": 2}
    ]}

    T0: 2 vehicles charging at 150kW each (300kW total)
    T1: 3rd vehicle arrives and starts charging, accepting up to 150kW
    T2: 4th vehicle arrives and starts charging, accepting up to 150kW
    T3: 1st vehicle finishes charging leaves
    Expected: Power reallocation without grid violation
    """
    # Set station configuration for scenario 2
    print_section("Station Configuration")
    _ = client.set_station_config(
        {
            "stationId": "TEST_STATION_SCENARIO_2",
            "gridCapacity": 400,
            "chargers": [
                {"id": "CP001", "maxPower": 300, "connectors": 2},
                {"id": "CP002", "maxPower": 300, "connectors": 2},
            ],
        }
    )
    config = client.get_station_config()
    print(f"   Station Config: {config}")

    print_section("T0: Starting Two Vehicles (150kW each)")

    # Start first vehicle on CP001:1
    print("\n🔌 Starting Vehicle 1 on CP001:1 (150kW max)")
    session1_response = client.create_session("CP001", 1, 150)
    session1 = session1_response["session"]
    print_session_info(session1, "Vehicle 1")

    time.sleep(0.5)

    # Start second vehicle on CP001:2
    print("\n🔌 Starting Vehicle 2 on CP001:2 (150kW max)")
    session2_response = client.create_session("CP001", 2, 150)
    session2 = session2_response["session"]
    print_session_info(session2, "Vehicle 2")

    print("\n🔄 Triggering power reallocation for vehicles...")
    for session, name in [
        (session1, "Vehicle 1"),
        (session2, "Vehicle 2"),
    ]:
        updated = client.power_update(session["sessionId"], session["allocatedPower"])
        session["allocatedPower"] = updated["session"]["allocatedPower"]
        session["vehicleMaxPower"] = updated["session"]["vehicleMaxPower"]
        print(f"   {name} reallocated to: {updated['session']['allocatedPower']}kW")

    print("\n📊 Status after T0:")
    client.print_station_status()

    print_section("T1: Third Vehicle Arrives (150kW max)")

    # Start third vehicle on CP002:1
    print("\n🔌 Starting Vehicle 3 on CP002:1 (150kW max)")
    session3_response = client.create_session("CP002", 1, 150)
    session3 = session3_response["session"]
    print_session_info(session3, "Vehicle 3")

    print("\n🔄 Triggering power reallocation for vehicles...")
    for session, name in [
        (session1, "Vehicle 1"),
        (session2, "Vehicle 2"),
        (session3, "Vehicle 3"),
    ]:
        updated = client.power_update(session["sessionId"], session["allocatedPower"])
        session["allocatedPower"] = updated["session"]["allocatedPower"]
        session["vehicleMaxPower"] = updated["session"]["vehicleMaxPower"]
        print(f"   {name} reallocated to: {updated['session']['allocatedPower']}kW")

    print("\n📊 Status after T1:")
    client.print_station_status()

    print_section("T2: Fourth Vehicle Arrives (150kW max)")

    # Start fourth vehicle on CP002:2
    print("\n🔌 Starting Vehicle 4 on CP002:2 (150kW max)")
    session4_response = client.create_session("CP002", 2, 150)
    session4 = session4_response["session"]
    print_session_info(session4, "Vehicle 4")

    print("\n🔄 Triggering power reallocation for vehicles...")
    for session, name in [
        (session1, "Vehicle 1"),
        (session2, "Vehicle 2"),
        (session3, "Vehicle 3"),
        (session4, "Vehicle 4"),
    ]:
        updated = client.power_update(session["sessionId"], session["allocatedPower"])
        session["allocatedPower"] = updated["session"]["allocatedPower"]
        session["vehicleMaxPower"] = updated["session"]["vehicleMaxPower"]
        print(f"   {name} reallocated to: {updated['session']['allocatedPower']}kW")

    print("\n📊 Status after T2 (all 4 vehicles charging):")
    client.print_station_status()

    print_section("T3: First Vehicle Finishes and Leaves")

    # Stop first vehicle session
    print(f"\n🔌 Vehicle 1 finishes charging and leaves...")
    client.stop_session(session1["sessionId"])
    print(f"   Stopped session {session1['sessionId'][:8]}...")

    time.sleep(0.5)

    # Update remaining vehicles to trigger reallocation
    print("\n🔄 Triggering power reallocation for vehicles...")
    for session, name in [
        (session2, "Vehicle 2"),
        (session3, "Vehicle 3"),
        (session4, "Vehicle 4"),
    ]:
        updated = client.power_update(session["sessionId"], session["allocatedPower"])
        session["allocatedPower"] = updated["session"]["allocatedPower"]
        session["vehicleMaxPower"] = updated["session"]["vehicleMaxPower"]
        print(f"   {name} reallocated to: {updated['session']['allocatedPower']}kW")

    print("\n📊 Final Status after T3 (Vehicle 1 left, power reallocated):")
    client.print_station_status()


def main():
    parser = argparse.ArgumentParser(description="Test SEMS power sharing scenario")
    _ = parser.add_argument(
        "--url",
        default="http://localhost:3000",
        help="Base URL of the SEMS server (default: http://localhost:3000)",
    )
    _ = parser.add_argument(
        "--scenario",
        default="1",
        help="Scenario number to run (1 or 2) (default: 1)",
    )

    args = parser.parse_args()

    client = SEMSClient(args.url)

    print_banner("SEMS Power Sharing Scenario Test")
    print(f"Server URL: {args.url}")

    # Health check
    print("\n🏥 Checking server health...")
    if not client.health_check():
        print("❌ Server is not responding. Please start the SEMS server first.")
        print("\nTo start the server:")
        print("  docker compose up")
        return 1
    print("✅ Server is healthy")

    try:
        if args.scenario == "1":
            scenario_1(client)
        elif args.scenario == "2":
            scenario_2(client)
        else:
            print(f"❌ Unknown scenario: {args.scenario}")
            return 1
        print("\n✅ Test completed successfully!")
        return 0
    except Exception as e:
        print(f"\n❌ Test failed with error: {e}")
    finally:
        print("\n🧹 Cleaning up any remaining sessions...")
        status = client.get_station_status()
        for session_id in status.get("sessions", {}):
            client.stop_session(session_id)
            print(f"   Stopped session {session_id[:8]}...")


if __name__ == "__main__":
    exit(main())
