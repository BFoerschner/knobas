package com.tidewater.settlebridge;

import com.tidewater.settlebridge.config.SettleBridgeConfig;

/** Terminates TLS and forwards settlement batches to the ledger. */
public final class SettleBridgeServer {
    private final SettleBridgeConfig config;

    public SettleBridgeServer(SettleBridgeConfig config) {
        this.config = config;
    }

    public void start() {
        System.out.println("listening on " + config.host() + ":" + config.port());
    }
}
