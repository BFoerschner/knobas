package com.tidewater.settlebridge.config;

public record SettleBridgeConfig(String host, int port) {
    public static SettleBridgeConfig fromEnv() {
        return new SettleBridgeConfig(System.getenv("GATEWAY_HOST"), Integer.parseInt(System.getenv("GATEWAY_PORT")));
    }
}
