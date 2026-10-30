# ProDeck

Deck material estimator for contractors and project managers. Describe the deck (one or more
rectangular sections, stairs, materials) and get:

- a **pick list** for Home Depot / Lowe's, grouped by department, that you can check off on a phone in the aisle
- an optimized **cut list** showing which stock board each piece comes from (least lumber bought, then fewest boards)
- the **framing design** behind it (joist/beam sizes, post spacing, footing sizes) with the reason for each choice

Everything is available through a documented REST API (`/api/docs`); the web app is just one client of it.

## Layout

| Path | What |
|---|---|
| `crates/estimator` | Pure calculation engine: geometry, DCA6 span tables, framing, decking, stairs, railing, cut optimizer, BOM |
| `crates/api-types` | REST request/response types shared by server and web client (the API contract) |
| `crates/server` | Axum server: `/api/v1`, OpenAPI at `/api/openapi.json`, Swagger UI at `/api/docs`, serves the web client |
| `crates/web` | Leptos (WASM, client-side rendered) mobile-first web client, built with Trunk |
| `infra/` | Terraform for GCP (Cloud Run, Firestore, Artifact Registry, Secret Manager, GitHub OIDC) |
| `.github/workflows` | CI (fmt, clippy, tests, terraform validate) and deploy-on-main |

## Local development

Prerequisites: Rust stable, `rustup target add wasm32-unknown-unknown`, `cargo install trunk --locked`.

```bash
# Terminal 1: API server with an in-memory store (data resets on restart)
STORE=memory COOKIE_SECURE=false \
BOOTSTRAP_ADMIN_EMAIL=admin@example.com BOOTSTRAP_ADMIN_PASSWORD=change-me-now \
cargo run -p server

# Terminal 2: web client with hot reload on http://localhost:8081 (proxies /api to :8080)
cd crates/web && trunk serve
```

Against the Firestore emulator instead: `gcloud emulators firestore start --host-port=localhost:8085`, then run the
server with `STORE=firestore GOOGLE_CLOUD_PROJECT=demo-prodeck FIRESTORE_EMULATOR_HOST=localhost:8085`.

Tests: `cargo test` (engine, scenarios, and API tests against the in-memory store).
Web client check: `cargo clippy -p web --target wasm32-unknown-unknown`.

Try the engine from the command line:

```bash
cargo run -p estimator --example dump -- crates/estimator/tests/fixtures/l_shape_two_level.json
```

## API

- Sign in: `POST /api/v1/auth/login` (sets an HttpOnly session cookie).
- API keys: create under **Settings > API Keys** (or `POST /api/v1/api-keys` from a signed-in session). Send
  `Authorization: Bearer pdk_<id>_<secret>`. Every key expires (max 365 days), is shown once, is stored only as a
  hash, and stops working when revoked or when its owner is disabled. API keys can't create keys or change passwords.
- Stateless estimate: `POST /api/v1/estimate` with a `Project` body. All lengths are inches.
- Full reference: `/api/docs` on any running instance.

## Deploying to GCP

One-time setup (needs `gcloud` and `terraform`, and Owner on the project):

```bash
PROJECT=your-gcp-project
gcloud storage buckets create gs://$PROJECT-tfstate --project=$PROJECT --location=us-central1 --uniform-bucket-level-access
cd infra
cp terraform.tfvars.example terraform.tfvars   # fill in project_id and bootstrap_admin_email
terraform init -backend-config="bucket=$PROJECT-tfstate"
terraform apply
```

Then set these **GitHub repository variables** (Settings > Secrets and variables > Actions > Variables) from the
Terraform outputs: `GCP_PROJECT_ID`, `GCP_REGION`, `GCP_IMAGE_REPO` (`image_repository`), `GCP_WIF_PROVIDER`
(`workload_identity_provider`), `GCP_DEPLOY_SA` (`deployer_service_account`). No JSON keys are needed.

Every push to `main` runs CI, builds the container, pushes it to Artifact Registry, deploys to Cloud Run, and
smoke-tests `/health`.

First sign-in: the admin email from `terraform.tfvars`, with the password from
`terraform output -raw bootstrap_admin_password_command | sh`. Change it under Settings.

### Custom domain (Cloudflare)

1. Verify the domain (or parent domain) in [Google Search Console](https://search.google.com/search-console) with the
   Google account that runs Terraform.
2. Set `domain = "deck.example.com"` in `terraform.tfvars` and `terraform apply`.
3. Create the records from `terraform output cloudflare_dns_records` in Cloudflare (normally a `CNAME` to
   `ghs.googlehosted.com`) with **Proxy status: DNS only** (grey cloud) so Google can issue the certificate.
   Certificate issuance can take 15-60 minutes.

### Cost

Cloud Run scales to zero and bills CPU only during requests; Firestore and Artifact Registry stay inside their free
tiers at this scale (image cleanup keeps the newest 5). Expect roughly $0-2/month. There is no load balancer.

## Engineering caveats

The engine follows the AWC DCA6 prescriptive approach (40 psf live / 10 psf dead; higher snow loads reduce spans
conservatively and are flagged). **Verify `crates/estimator/data/span_tables.toml` against the DCA6 edition your
jurisdiction adopts before using estimates for permits.** Output is a material estimate; local code and the
inspector have final say.
