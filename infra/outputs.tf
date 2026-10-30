output "service_url" {
  value = google_cloud_run_v2_service.app.uri
}

output "image_repository" {
  description = "Push images here (GitHub variable GCP_IMAGE_REPO)."
  value       = local.image_repo
}

output "workload_identity_provider" {
  description = "GitHub variable GCP_WIF_PROVIDER."
  value       = google_iam_workload_identity_pool_provider.github.name
}

output "deployer_service_account" {
  description = "GitHub variable GCP_DEPLOY_SA."
  value       = google_service_account.deployer.email
}

output "bootstrap_admin_password_command" {
  description = "Read the first admin's password (change it after signing in)."
  value       = "gcloud secrets versions access latest --secret=${google_secret_manager_secret.bootstrap_admin_password.secret_id} --project=${var.project_id}"
}

output "cloudflare_dns_records" {
  description = "Create these in Cloudflare with the proxy OFF (DNS only)."
  value       = var.domain == "" ? [] : google_cloud_run_domain_mapping.app[0].status[0].resource_records
}
