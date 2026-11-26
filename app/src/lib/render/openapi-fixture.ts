/**
 * OpenAPI specs for the tests (ux18): one per dialect and format, a cycle, a split spec, a broken one, a YAML file
 * that is no spec, and a generated spec of any size (5,000 operations for the windowing and timing tests).
 */

/** OpenAPI 3.0, YAML: tags, a path parameter at the path, a body by ref, a ref cycle (`Pet.parent`). */
export const PETS_30_YAML = `openapi: 3.0.3
info:
  title: Pet Store
  version: 1.4.0
  description: |
    The **pet** store.
servers:
  - url: https://api.example.com/v1
    description: Production
tags:
  - name: pets
    description: Everything about pets
  - name: store
paths:
  /pets:
    get:
      tags: [pets]
      summary: List all pets
      operationId: listPets
      parameters:
        - name: limit
          in: query
          description: How many to return
          schema:
            type: integer
            format: int32
      responses:
        "200":
          description: A page of pets
          headers:
            x-next:
              description: A link to the next page
              schema:
                type: string
          content:
            application/json:
              schema:
                type: array
                items:
                  $ref: "#/components/schemas/Pet"
        default:
          $ref: "#/components/responses/Error"
    post:
      tags: [pets]
      summary: Create a pet
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Pet"
            example:
              id: 1
              name: Rex
      responses:
        "201":
          description: Created
  /pets/{petId}:
    parameters:
      - name: petId
        in: path
        required: true
        schema:
          type: string
    get:
      tags: [pets]
      summary: Info for a specific pet
      operationId: showPetById
      responses:
        "200":
          description: The pet
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Pet"
    delete:
      tags: [pets]
      summary: Remove a pet
      deprecated: true
      responses:
        "204":
          description: Gone
  /store/inventory:
    get:
      tags: [store]
      summary: Inventory by status
      responses:
        "200":
          description: Counts
  /health:
    get:
      summary: Health check
      responses:
        "200":
          description: Up
components:
  responses:
    Error:
      description: Something went wrong
      content:
        application/json:
          schema:
            $ref: "#/components/schemas/Error"
  schemas:
    Pet:
      type: object
      description: A pet of the store
      required: [id, name]
      properties:
        id:
          type: integer
          format: int64
        name:
          type: string
        status:
          type: string
          enum: [available, sold]
        parent:
          $ref: "#/components/schemas/Pet"
        owner:
          $ref: "#/components/schemas/Owner"
    Owner:
      type: object
      properties:
        name:
          type: string
        pets:
          type: array
          items:
            $ref: "#/components/schemas/Pet"
    Error:
      type: object
      properties:
        code:
          type: integer
        message:
          type: string
`;

/** OpenAPI 3.1, YAML: `type: [string, "null"]`, webhooks only (3.1 may have no paths), a ref to another file. */
export const SPLIT_31_YAML = `openapi: 3.1.0
info:
  title: Split API
  version: "2"
paths:
  /orders:
    get:
      summary: List orders
      responses:
        "200":
          description: Orders
          content:
            application/json:
              schema:
                $ref: "./schemas/order.yaml#/Order"
  /items:
    $ref: "./paths/items.yaml"
  /far:
    get:
      summary: A ref to a URL
      responses:
        "200":
          description: Far
          content:
            application/json:
              schema:
                $ref: "https://example.com/schemas/far.json"
  /out:
    get:
      summary: A ref out of the repository
      responses:
        "200":
          description: Out
          content:
            application/json:
              schema:
                $ref: "../../../outside.yaml#/X"
`;

export const ORDER_YAML = `Order:
  type: object
  properties:
    id:
      type: [string, "null"]
    lines:
      type: array
      items:
        $ref: "#/Line"
Line:
  type: object
  properties:
    sku:
      type: string
`;

export const ITEMS_YAML = `get:
  summary: List items
  responses:
    "200":
      description: Items
`;

/** Swagger 2.0, JSON: a body parameter, `produces`, `definitions`. */
export const STORE_20_JSON = JSON.stringify(
  {
    swagger: "2.0",
    info: { title: "Old Store", version: "0.9" },
    host: "store.example.com",
    basePath: "/api",
    schemes: ["https"],
    produces: ["application/json"],
    paths: {
      "/orders": {
        post: {
          summary: "Place an order",
          tags: ["orders"],
          parameters: [
            { name: "body", in: "body", required: true, schema: { $ref: "#/definitions/Order" } },
            { name: "X-Trace", in: "header", type: "string" },
          ],
          responses: { "200": { description: "Placed", schema: { $ref: "#/definitions/Order" }, examples: { "application/json": { id: 7 } } } },
        },
      },
    },
    definitions: { Order: { type: "object", properties: { id: { type: "integer" }, qty: { type: "integer", minimum: 1 } } } },
  },
  null,
  2,
);

/** OpenAPI 3.0, JSON. */
export const TINY_30_JSON = `{
  "openapi": "3.0.0",
  "info": { "title": "Tiny", "version": "1" },
  "paths": { "/ping": { "get": { "summary": "Ping", "responses": { "200": { "description": "Pong" } } } } }
}`;

/** Looks like a spec at its top, then stops parsing on line 6. */
export const BROKEN_YAML = `openapi: 3.0.0
info:
  title: Broken
  version: 1
paths:
  /a: [unclosed
`;

/** A YAML file that is no spec. */
export const COMPOSE_YAML = `services:
  web:
    image: nginx
    environment:
      openapi: 3.0.0
`;

/** A spec of `n` operations, YAML: `n / 4` paths of four methods, in 25 tags, one schema per path. */
export function bigSpec(n: number): string {
  const lines = ["openapi: 3.0.3", "info:", "  title: Big", "  version: 1.0.0", "paths:"];
  const methods = ["get", "post", "put", "delete"];
  for (let p = 0; p < Math.ceil(n / 4); p++) {
    lines.push(`  /resource${p}/{id}:`);
    for (let m = 0; m < 4 && p * 4 + m < n; m++) {
      lines.push(
        `    ${methods[m]}:`,
        `      tags: [tag${p % 25}]`,
        `      summary: ${methods[m]} resource ${p}`,
        `      operationId: op${p}_${methods[m]}`,
        "      parameters:",
        "        - name: id",
        "          in: path",
        "          required: true",
        "          schema: { type: string }",
        "      responses:",
        '        "200":',
        "          description: OK",
        "          content:",
        "            application/json:",
        `              schema: { $ref: "#/components/schemas/Thing${p}" }`,
      );
    }
  }
  lines.push("components:", "  schemas:");
  for (let p = 0; p < Math.ceil(n / 4); p++) {
    lines.push(`    Thing${p}:`, "      type: object", "      properties:", "        id: { type: string }", `        next: { $ref: "#/components/schemas/Thing${(p + 1) % Math.ceil(n / 4)}" }`);
  }
  return `${lines.join("\n")}\n`;
}

/**
 * OpenAPI 3.1, JSON (ux31): what the worker reads now. Servers with a variable, an operation's own servers and a
 * path item's, security schemes of each kind with `[{}]` and a requirement of two schemes, a response's links, a
 * callback, external docs, a webhook, contact, licence and terms.
 */
export const MODEL_31_JSON = JSON.stringify(
  {
    openapi: "3.1.0",
    info: {
      title: "Model",
      version: "2",
      contact: { name: "API team", email: "api@example.test", url: "https://example.test/team" },
      license: { name: "MIT", url: "https://example.org/licenses/MIT" },
      termsOfService: "https://example.test/terms",
    },
    externalDocs: { url: "https://example.test/docs", description: "Guide" },
    servers: [{ url: "https://" + "{tenant}.example.test/{base}", description: "Hosted", variables: { tenant: { default: "acme", enum: ["acme", "globex"] }, base: { default: "v2" } } }],
    security: [{ bearer: [] }],
    paths: {
      "/things": {
        servers: [{ url: "https://things.example.test" }],
        get: {
          summary: "List things",
          operationId: "listThings",
          security: [{}],
          responses: {
            "200": {
              description: "Things",
              links: { first: { operationId: "getThing", description: "The first thing" }, ref: { $ref: "#/components/links/Next" } },
              content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/Thing" } } } },
            },
          },
        },
        post: {
          summary: "Make a thing",
          servers: [{ url: "https://write.example.test" }],
          security: [{ key: [] }, { basic: [], key: [] }],
          callbacks: { made: { "{$request.body#/hook}": { post: { responses: { "200": { description: "OK" } } } } } },
          externalDocs: { url: "https://example.test/make" },
          requestBody: { content: { "application/json": { schema: { $ref: "#/components/schemas/Thing" } } } },
          responses: { "201": { description: "Made" } },
        },
      },
      "/things/{id}": {
        get: { operationId: "getThing", parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }], responses: { "200": { description: "A thing" } } },
      },
    },
    webhooks: { thingMade: { post: { summary: "A thing was made", requestBody: { content: { "application/json": { schema: { $ref: "#/components/schemas/Thing" } } } }, responses: { "200": { description: "Taken" } } } } },
    components: {
      links: { Next: { operationId: "listThings", description: "More" } },
      securitySchemes: {
        bearer: { type: "http", scheme: "bearer", bearerFormat: "JWT" },
        basic: { type: "http", scheme: "Basic" },
        key: { type: "apiKey", in: "query", name: "api_key", description: "From your settings" },
        oauth: { type: "oauth2", flows: { authorizationCode: { authorizationUrl: "https://example.test/a", tokenUrl: "https://example.test/t", scopes: {} } } },
        oidc: { type: "openIdConnect", openIdConnectUrl: "https://example.test/.well-known/openid-configuration" },
      },
      schemas: {
        Thing: { type: "object", required: ["id"], properties: { id: { type: "string", readOnly: true }, name: { type: "string", example: "lamp" }, made: { type: "string", format: "date-time" } } },
      },
    },
  },
  null,
  2,
);

/** Swagger 2.0, JSON (ux31): `formData` parameters with a file, a 2.0 `basic` and an `apiKey` scheme, no host. */
export const FORM_20_JSON = JSON.stringify(
  {
    swagger: "2.0",
    info: { title: "Upload", version: "1" },
    basePath: "/v2",
    securityDefinitions: { basic: { type: "basic" }, key: { type: "apiKey", in: "header", name: "X-API-Key" } },
    paths: {
      "/pets/{id}/photo": {
        post: {
          parameters: [
            { name: "id", in: "path", required: true, type: "integer" },
            { name: "caption", in: "formData", type: "string", description: "Words under it" },
            { name: "file", in: "formData", type: "file", required: true },
          ],
          responses: { "200": { description: "Taken" } },
        },
      },
      "/pets/{id}": {
        post: {
          parameters: [
            { name: "id", in: "path", required: true, type: "integer" },
            { name: "name", in: "formData", type: "string", required: true },
          ],
          responses: { "200": { description: "Changed" } },
        },
      },
    },
  },
  null,
  2,
);

/**
 * OpenAPI 3.0, JSON (ux31): every path under `/v1` and no tags, as Stripe writes it, with `anyOf [X, ""]` fields
 * and `allOf` and `oneOf` three deep. The fake's fixture for today's failures (section 30).
 */
export const DEEP_30_JSON = JSON.stringify(
  {
    openapi: "3.0.3",
    info: { title: "Payments", version: "2026-11-01", description: "Every path under `/v1`, no tags." },
    servers: [{ url: "https://api.payments.test" }],
    components: {
      securitySchemes: { basicAuth: { type: "http", scheme: "basic" }, bearerAuth: { type: "http", scheme: "bearer" } },
      schemas: {
        Base: { type: "object", required: ["id"], properties: { id: { type: "string", description: "The id." }, object: { type: "string" } } },
        Named: { allOf: [{ $ref: "#/components/schemas/Base" }, { type: "object", required: ["name"], properties: { name: { type: "string", maxLength: 255 } } }] },
        Card: { allOf: [{ $ref: "#/components/schemas/Named" }, { type: "object", properties: { last4: { type: "string", pattern: "^[0-9]{4}$" } } }] },
        Bank: { allOf: [{ $ref: "#/components/schemas/Named" }, { type: "object", properties: { iban: { type: "string" } } }] },
        Charge: {
          allOf: [
            { $ref: "#/components/schemas/Base" },
            {
              type: "object",
              required: ["amount"],
              properties: {
                amount: { type: "integer", minimum: 1 },
                customer: { anyOf: [{ type: "string" }, { $ref: "#/components/schemas/Customer" }], description: "The customer, or its id." },
                description: { anyOf: [{ type: "string", maxLength: 5000 }, { type: "string", enum: [""] }] },
                source: { oneOf: [{ $ref: "#/components/schemas/Card" }, { $ref: "#/components/schemas/Bank" }], discriminator: { propertyName: "object" } },
                parent: { $ref: "#/components/schemas/Charge" },
              },
            },
          ],
        },
        Customer: { allOf: [{ $ref: "#/components/schemas/Named" }], nullable: true },
      },
    },
    security: [{ basicAuth: [] }, { bearerAuth: [] }],
    paths: {
      "/v1/charges": {
        get: { summary: "List charges", responses: { "200": { description: "Charges", content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/Charge" } } } } } } },
        post: {
          summary: "Create a charge",
          requestBody: { required: true, content: { "application/x-www-form-urlencoded": { schema: { $ref: "#/components/schemas/Charge" } }, "application/json": { schema: { $ref: "#/components/schemas/Charge" } } } },
          responses: { "200": { description: "The charge", content: { "application/json": { schema: { $ref: "#/components/schemas/Charge" } } } }, default: { description: "An error" } },
        },
      },
      "/v1/charges/{charge}": { get: { summary: "Retrieve a charge", parameters: [{ name: "charge", in: "path", required: true, schema: { type: "string" } }], responses: { "200": { description: "The charge" } } } },
      "/v1/customers": { get: { summary: "List customers", responses: { "200": { description: "Customers" } } } },
      "/v1/customers/{customer}": { delete: { summary: "Delete a customer", deprecated: true, parameters: [{ name: "customer", in: "path", required: true, schema: { type: "string" } }], responses: { "200": { description: "Gone" } } } },
      "/v1/payouts": { post: { summary: "Create a payout", responses: { "200": { description: "The payout" } } } },
    },
  },
  null,
  2,
);
