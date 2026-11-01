# Online orders: setting up

**Status:** 2026-10-25, first version (ORD-01). **Revision history:** 2026-10-25 first version. 2026-11-01: CSV lines known by order, product and line id instead of row; designs with the same name on one order no longer overwrite each other (ORD-02).

StitchFoundry can read the orders from your online shop and make each personalized item into a
design on its own. It takes the name or text the customer typed and sets it in the font and
thread colour your rules give. Each item also gets a job order, its machine files and a worksheet
with a barcode. This page explains how to connect a shop. Open the window with **Embroidery ›
Online Orders…** (`ui.onlineOrders`).

Only what the shop's own public documentation says is used here; links are given at each step.

## 1. Pick your shop

Under **Your shop**, choose one:

- **Shopify** reads your store's unfulfilled orders with the Admin API.
- **WooCommerce** reads the "processing" orders from your WordPress site with the WooCommerce
  REST API.
- **Another shop (CSV file)** works with Etsy, Squarespace, Wix or anything else that exports
  orders as a spreadsheet. You download the file, then press **Make Designs from a CSV File…**.

The keys you type (client ID and secret, or consumer key and secret) are kept in your computer's
own password store (Keychain, Windows Credential Manager or the Secret Service). They are never
written to StitchFoundry's settings file or sent anywhere except your shop.

## 2a. Shopify

Since 1 January 2026, new apps for your own store are made in Shopify's Dev Dashboard rather
than in the store's admin
([Shopify: custom apps](https://help.shopify.com/en/manual/apps/app-types/custom-apps)).

1. Sign in to the **Dev Dashboard** (<https://dev.shopify.com/dashboard/>) with the same
   Shopify account and organization that owns your store.
2. **Create app** and give it a name, for example "StitchFoundry".
3. In the app's **Configuration** (access scopes), add **`read_orders`**. To show the customer's
   name on jobs and worksheets, add **`read_customers`** too. Without it StitchFoundry uses the
   shipping name or e-mail instead.
4. **Release** a version of the app, then **Install** it on your store.
5. On the app's **Settings** page, copy the **Client ID** and the **Client secret**.
6. In StitchFoundry, type your store's address (`yourshop.myshopify.com`, the address Shopify
   gave you, not your own domain) and paste the client ID and secret.

How it signs in: StitchFoundry exchanges the client ID and secret for an access token with
Shopify's client credentials grant. The token lasts 24 hours, and a new one is fetched when
needed
([Shopify: client credentials grant](https://shopify.dev/docs/apps/build/authentication-authorization/access-tokens/client-credentials-grant)).
This only works when the app and the store belong to the **same organization**. An app made
under a different account gets a 400 or 401 answer.

It then asks the GraphQL Admin API (version 2026-10) for the 50 newest open, unfulfilled orders
([Shopify: GraphQL Admin API](https://shopify.dev/docs/api/admin-graphql),
[orders query](https://shopify.dev/docs/api/admin-graphql/latest/queries/orders)). The text
comes from the line items' custom attributes: the "line item properties" a personalization field
on your product page fills in
([Shopify: line item properties](https://shopify.dev/docs/storefronts/themes/architecture/templates/product#line-item-properties)).

## 2b. WooCommerce

1. In WordPress, go to **WooCommerce › Settings › Advanced › REST API › Add key**
   ([WooCommerce: REST API keys](https://woocommerce.com/document/woocommerce-rest-api/)).
2. Give it a description ("StitchFoundry"), choose your user, and set **Permissions** to
   **Read**.
3. **Generate API key**, then copy the **Consumer key** (`ck_…`) and **Consumer secret**
   (`cs_…`). WooCommerce shows the secret only once.
4. In StitchFoundry, type your site's address (`https://example.com`) and paste both keys.

Your site must use **HTTPS**: the keys are sent with every request, so StitchFoundry refuses a
plain `http://` address. WordPress's **Settings › Permalinks** must be anything but "Plain", or
`/wp-json/` isn't found (a 404).

StitchFoundry reads `GET /wp-json/wc/v3/orders?status=processing`
([WooCommerce REST API docs: orders](https://woocommerce.github.io/woocommerce-rest-api-docs/#list-all-orders)).
The text comes from each line item's meta data: the fields a product add-on or personalization
plug-in adds to the item.

## 2c. Etsy and other shops (CSV)

Download the orders as a CSV file. On Etsy, that's **Shop Manager › Settings › Options ›
Download Data › Orders**, choosing the "Order Items" type
([Etsy: download a spreadsheet of your sold transactions](https://help.etsy.com/hc/articles/360000343328)). Etsy's Order Items file has Order ID, Item Name, Buyer, Quantity, SKU and Variations columns, which StitchFoundry reads as they are.
Then press **Make Designs from a CSV File…**.

StitchFoundry finds the columns by their header names:

| What | Header names it looks for |
|---|---|
| Order number | Order, Order ID, Order Number, Order No, Order #, Name |
| Customer | Customer, Customer Name, Buyer, Full Name, Ship Name, Shipping Name, Billing Name |
| Product | Product, Product Name, Item, Item Name, Lineitem Name, Title |
| SKU | SKU, Lineitem SKU, Item SKU |
| Quantity | Quantity, Qty, Lineitem Quantity |
| Line id (optional) | Transaction ID, Line Item ID, Lineitem ID, Order Item ID |

A CSV line is known again by its order number and product, and by its line id when the file has
one (else by which of that order's lines of that product it is), never by its row. Etsy's file
lists the newest orders first, so yesterday's orders move down a row each time a new one comes
in; they still aren't made twice.

Every other column counts as an order field. A **Variations** or **Options** column such as
Etsy's (`Personalization:Rose, Colour:Navy`) is split into its fields. Commas, semicolons and
tabs all work as separators.

## 3. Rules

Rules say how an item becomes a design. **The first rule whose product matches makes the
item.** Leave **Product** empty to take any item.

| Setting | What it does |
|---|---|
| **Product** | Text found in the product's name or SKU, such as "towel" or "CAP-" |
| **Template** | A design template (a name tape, a left chest, a cap front…); the text is fitted to its area |
| **Font** | The embroidery font, unless the order has a font field (named "font") |
| **Thread** | The thread colour, unless the order has a colour field (named "colour" or "color"); a name ("Navy", "Red") or `#rrggbb` |
| **Size (mm)** | The letters' height; empty means as large as fits the template |

Which field holds the text: by default, any field whose name says *name*, *text*,
*personalization*, *monogram*, *initials*, *line*, *wording*, *message* or *embroidery*. Fields
about the font, colour, size, placement, e-mail, date or phone are left out. Several lines (Line
1, Line 2…) become several lines of lettering, up to four. A rule can name the fields exactly
instead (`textFields` in `embroidery.orders.setup`).

Items with no text, or that no rule matches, aren't made. They're listed under the results with
the reason, so you can look at them by hand.

## 4. What is made, and where

Each item becomes:

- a design in **Save designs in** (My Designs/Orders unless you choose another folder), named
  after the order number and its first line of text; when that name is taken (a towel and a bib
  on one order both saying "Rose"), the next design gets " 2", " 3"… added, so no design is ever
  overwritten;
- a **job order** carrying the shop's order number, the customer, the quantity and the product
  (Embroidery › Jobs);
- the **machine files** you tick under **Machine files** (DST, PES, JEF…; none means just the
  design and worksheet);
- a **worksheet** with the job's barcode.

Each order line is made only once. StitchFoundry remembers the lines it has made, so pressing
**Check for New Orders** again only makes the new ones. **Check every 15 minutes while open**
does it on its own.

**Find a job:** scan a worksheet's barcode with a USB barcode scanner (it types like a keyboard
and presses Enter), or type the job number (J-0007) or the shop's order number (#1001). The job
opens.

## 5. When something goes wrong

| What you see | What it means |
|---|---|
| 400 or 401 from Shopify | Wrong client ID or secret, or the app belongs to a different organization than the store |
| 403 from Shopify | The app lacks `read_orders`, or the version with it wasn't released and installed |
| 401 from WooCommerce | Wrong consumer key or secret, or the key was revoked |
| 403 from WooCommerce | The key's permission isn't Read (or Read/Write) |
| 404 | The store address is wrong; for WooCommerce, permalinks are set to "Plain" |
| 429 | The shop asks for fewer requests: wait a minute and check again |
| "{n} designs made, {m} to look at" | The listed items had no text, or no rule matched them |

## 6. For scripts and automation

Everything the window does is a command, reachable from the control channel and MCP
(`docs/mcp.md`):

- `embroidery.orders.setup {shop, store, folder, formats, rules, forget}` sets the shop and rules.
- `embroidery.orders.import {source, data, dryRun}` makes the designs from a shop's answer or a
  CSV file's text. With `dryRun` it only lists what would be made.
- `embroidery.job.find {code}` finds a job by job or order number.

The keys aren't part of any command, so the window (or your own script) does the fetching and
passes the answer to `embroidery.orders.import`.
