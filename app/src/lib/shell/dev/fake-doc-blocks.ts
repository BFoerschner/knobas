/**
 * `?fake-ipc`'s XML, JSON and schema texts for the Docs blocks and the file viewer (D1, the XML view's design section
 * 28): the code blocks of *EDI message formats* (`docs.ts`) and two of PAY-231's attachments (`attachments.ts`).
 *
 * - **A schema**: UN/CEFACT's Unqualified Data Type, D16B, as published (`fixtures/xsd/cii-d16b/`): 160 lines, 14
 *   complex types. It offers *Schema* at any length.
 * - **An XML document of 61 lines** and **a JSON document of 61 lines**: each offers *XML view* or *JSON view*.
 * - **An XML document of 14 lines**, under the 40 a block needs before it offers its view: it stays plain code.
 * - **A run's `junit.xml`** (46 lines) and **a JSON answer** (46 lines), the attachments the file viewer folds.
 */
import UNQUALIFIED_DATA_TYPE from "../../../../../fixtures/xsd/cii-d16b/data/standard/CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd?raw";

export const SCHEMA_BLOCK = UNQUALIFIED_DATA_TYPE.trimEnd();

/** An export declaration as the customs gateway sends it: 61 lines. */
export const XML_BLOCK = `<?xml version="1.0" encoding="UTF-8"?>
<ExportDeclaration xmlns="urn:tidewater:customs:export:3" version="3.2">
  <Header>
    <MessageId>EXD-2026-08-0419</MessageId>
    <Created>2026-08-19T09:12:44Z</Created>
    <Office code="DE004851">Hamburg Waltershof</Office>
    <Procedure code="10" additional="000"/>
  </Header>
  <Declarant eori="DE7712340000123">
    <Name>Tidewater Freight GmbH</Name>
    <Address>
      <Street>Am Sandtorkai 41</Street>
      <City>Hamburg</City>
      <Postcode>20457</Postcode>
      <Country>DE</Country>
    </Address>
  </Declarant>
  <Consignee>
    <Name>Northwind Supply Ltd</Name>
    <Address>
      <Street>12 Dock Road</Street>
      <City>Felixstowe</City>
      <Postcode>IP11 3SY</Postcode>
      <Country>GB</Country>
    </Address>
  </Consignee>
  <Transport mode="1">
    <Vessel imo="9811000">MSC Isabella</Vessel>
    <Container>MSCU4411827</Container>
    <Seal>DE-HH-883104</Seal>
  </Transport>
  <Items>
    <Item sequence="1">
      <Description>Pallet racking, galvanised steel</Description>
      <HsCode>73089098</HsCode>
      <Origin>DE</Origin>
      <Packages kind="PX" count="12"/>
      <GrossMass unit="kg">4180.0</GrossMass>
      <NetMass unit="kg">3920.5</NetMass>
      <StatisticalValue currency="EUR">18450.00</StatisticalValue>
    </Item>
    <Item sequence="2">
      <Description>Forklift battery charger, 48 V</Description>
      <HsCode>85044095</HsCode>
      <Origin>CN</Origin>
      <Packages kind="CT" count="4"/>
      <GrossMass unit="kg">212.0</GrossMass>
      <NetMass unit="kg">188.4</NetMass>
      <StatisticalValue currency="EUR">6230.00</StatisticalValue>
    </Item>
    <Item sequence="3">
      <Description>Stretch film, 23 micron</Description>
      <HsCode>39201025</HsCode>
      <Origin>PL</Origin>
      <Packages kind="RO" count="40"/>
      <GrossMass unit="kg">640.0</GrossMass>
      <NetMass unit="kg">612.0</NetMass>
      <StatisticalValue currency="EUR">1904.40</StatisticalValue>
    </Item>
  </Items>
</ExportDeclaration>`;

/** The gateway's status hook for that declaration: 61 lines. */
export const JSON_BLOCK = `{
  "declaration": "EXD-2026-08-0419",
  "office": { "code": "DE004851", "name": "Hamburg Waltershof" },
  "status": "released",
  "mrn": "26DE004851000419E4",
  "history": [
    {
      "at": "2026-08-19T09:12:44Z",
      "status": "lodged",
      "by": "gateway"
    },
    {
      "at": "2026-08-19T09:13:02Z",
      "status": "accepted",
      "by": "atlas"
    },
    {
      "at": "2026-08-19T10:41:37Z",
      "status": "risk-checked",
      "by": "atlas",
      "result": { "lane": "green", "inspection": false }
    },
    {
      "at": "2026-08-19T10:41:38Z",
      "status": "released",
      "by": "atlas"
    }
  ],
  "items": [
    {
      "sequence": 1,
      "hsCode": "73089098",
      "duty": { "rate": 0, "amount": 0, "currency": "EUR" },
      "documents": ["N380", "Y923"]
    },
    {
      "sequence": 2,
      "hsCode": "85044095",
      "duty": { "rate": 0, "amount": 0, "currency": "EUR" },
      "documents": ["N380", "3200"]
    },
    {
      "sequence": 3,
      "hsCode": "39201025",
      "duty": { "rate": 0, "amount": 0, "currency": "EUR" },
      "documents": ["N380"]
    }
  ],
  "exit": {
    "office": "GB000060",
    "expected": "2026-08-21",
    "confirmed": null
  },
  "links": {
    "self": "https://customs-gateway.tidewater.internal/declarations/EXD-2026-08-0419",
    "pdf": "https://customs-gateway.tidewater.internal/declarations/EXD-2026-08-0419.pdf"
  },
  "warnings": [],
  "retries": 0,
  "received": "2026-08-19T10:41:39Z"
}`;

/** The gateway's acknowledgement: 14 lines, under the 40 a block needs before it offers its view. */
export const SHORT_XML_BLOCK = `<?xml version="1.0" encoding="UTF-8"?>
<Acknowledgement xmlns="urn:tidewater:customs:ack:1">
  <MessageId>ACK-2026-08-0419</MessageId>
  <Refers>EXD-2026-08-0419</Refers>
  <Received>2026-08-19T09:13:02Z</Received>
  <Result code="accepted"/>
  <Mrn>26DE004851000419E4</Mrn>
  <Office code="DE004851"/>
  <Notes>
    <Note lang="en">Lodged before the vessel's cut-off.</Note>
    <Note lang="de">Vor dem Annahmeschluss eingereicht.</Note>
  </Notes>
  <Signature algorithm="none"/>
</Acknowledgement>`;

/** A run's test report as JUnit writes it: one suite of eight cases, one failing, one skipped. */
export const JUNIT_XML = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="payout-service" tests="8" failures="1" errors="0" skipped="1" time="4.812">
  <testsuite name="RetrySchedulerTest" tests="8" failures="1" errors="0" skipped="1" time="4.812" timestamp="2026-08-14T08:41:12" hostname="ci-agent-03">
    <properties>
      <property name="java.version" value="21.0.4"/>
      <property name="spring.profiles.active" value="test"/>
      <property name="psp.sandbox" value="true"/>
    </properties>
    <testcase name="retriesTransientCodeWithBackoff" classname="io.tidewater.payout.RetrySchedulerTest" time="0.412"/>
    <testcase name="stopsAfterFiveAttempts" classname="io.tidewater.payout.RetrySchedulerTest" time="0.388"/>
    <testcase name="sendsRateLimitedToReview" classname="io.tidewater.payout.RetrySchedulerTest" time="0.297"/>
    <testcase name="keepsIdempotencyKeyAcrossRetries" classname="io.tidewater.payout.RetrySchedulerTest" time="1.204">
      <failure message="expected one payout for PO-130102, found 2" type="org.opentest4j.AssertionFailedError"><![CDATA[org.opentest4j.AssertionFailedError: expected one payout for PO-130102, found 2
	at io.tidewater.payout.RetrySchedulerTest.keepsIdempotencyKeyAcrossRetries(RetrySchedulerTest.java:142)
	at java.base/java.lang.reflect.Method.invoke(Method.java:580)
	at org.junit.platform.commons.util.ReflectionUtils.invokeMethod(ReflectionUtils.java:728)
	at org.junit.jupiter.engine.execution.MethodInvocation.proceed(MethodInvocation.java:60)
	at org.junit.jupiter.engine.execution.InvocationInterceptorChain$ValidatingInvocation.proceed(InvocationInterceptorChain.java:131)
	at org.junit.jupiter.engine.extension.TimeoutExtension.intercept(TimeoutExtension.java:156)
	at org.junit.jupiter.engine.descriptor.TestMethodTestDescriptor.invokeTestMethod(TestMethodTestDescriptor.java:218)
	at org.junit.platform.engine.support.hierarchical.NodeTestTask.executeRecursively(NodeTestTask.java:151)]]></failure>
      <system-out><![CDATA[08:41:14.201 INFO  RetryScheduler - PO-130102 attempt 1 failed: TEMP_UNAVAILABLE
08:41:14.233 INFO  RetryScheduler - PO-130102 attempt 2 scheduled in 30 s
08:41:14.236 WARN  PspClient - PO-130102 sent with key 7f3a-11 and key 7f3a-12]]></system-out>
    </testcase>
    <testcase name="jitterStaysWithinTenPercent" classname="io.tidewater.payout.RetrySchedulerTest" time="0.733"/>
    <testcase name="finishesBeforeCutoff" classname="io.tidewater.payout.RetrySchedulerTest" time="0.921"/>
    <testcase name="pagesOnCallAboveTenInReview" classname="io.tidewater.payout.RetrySchedulerTest" time="0.000">
      <skipped message="waiting for PAY-240"/>
    </testcase>
    <testcase name="readsRetiredCodesAsSuccessors" classname="io.tidewater.payout.RetrySchedulerTest" time="0.857">
      <system-out><![CDATA[08:41:16.402 INFO  PspCodes - BUSY read as TEMP_UNAVAILABLE
08:41:16.403 INFO  PspCodes - THROTTLED read as RATE_LIMITED]]></system-out>
    </testcase>
    <system-out><![CDATA[08:41:12.017 INFO  Spring - Started RetrySchedulerTest in 1.9 s
08:41:12.019 INFO  PspSandbox - IBANs ending in 0503 answer TEMP_UNAVAILABLE]]></system-out>
    <system-err><![CDATA[]]></system-err>
  </testsuite>
  <testsuite name="LedgerBookingTest" tests="0" failures="0" errors="0" skipped="0" time="0.000" timestamp="2026-08-14T08:41:17" hostname="ci-agent-03">
    <properties>
      <property name="java.version" value="21.0.4"/>
    </properties>
    <system-out><![CDATA[]]></system-out>
    <system-err><![CDATA[]]></system-err>
  </testsuite>
</testsuites>
`;

/** The PSP's answer to the retried payout, as the trace saved it. */
export const PSP_ANSWER_JSON = `{
  "request": {
    "method": "POST",
    "path": "/v2/payouts",
    "idempotencyKey": "7f3a-12",
    "sentAt": "2026-08-13T22:41:15.204Z"
  },
  "response": {
    "status": 409,
    "headers": {
      "content-type": "application/problem+json",
      "x-request-id": "psp-5c1e9a20",
      "retry-after": null
    },
    "body": {
      "type": "https://psp.example/problems/duplicate-payout",
      "title": "Duplicate payout",
      "status": 409,
      "detail": "A payout for this beneficiary and amount was accepted 31 s ago under key 7f3a-11.",
      "payout": {
        "id": "PO-130102",
        "iban": "FR1420041010050500013M02606",
        "amount": { "value": "3015.75", "currency": "EUR" },
        "acceptedAt": "2026-08-13T22:40:44.118Z",
        "key": "7f3a-11"
      },
      "codes": [
        { "code": "DUPLICATE", "final": true },
        { "code": "AC04", "final": true }
      ]
    }
  },
  "attempts": [
    { "n": 1, "at": "2026-08-13T22:40:44.090Z", "key": "7f3a-11", "status": 202 },
    { "n": 2, "at": "2026-08-13T22:41:15.204Z", "key": "7f3a-12", "status": 409 }
  ],
  "client": {
    "name": "payout-service",
    "version": "3.14.2",
    "retryPolicy": { "base": "30s", "factor": 2, "maxAttempts": 5, "jitter": 0.1 }
  },
  "notes": [
    "The second attempt carried a new key: the retry window was not idempotent.",
    "Fixed by keeping the first key across attempts (PAY-231)."
  ]
}
`;
