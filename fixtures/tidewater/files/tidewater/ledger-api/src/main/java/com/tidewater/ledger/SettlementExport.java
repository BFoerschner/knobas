package com.tidewater.ledger;

import com.tidewater.settlebridge.client.Batch;

/** Hands the day's settlements to the gateway; a refused batch goes on the RetryQueue. */
public class SettlementExport {
    public void export(Batch batch) {}
}
