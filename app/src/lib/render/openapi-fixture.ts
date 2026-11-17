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
