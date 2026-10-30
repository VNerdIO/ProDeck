variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "region" {
  description = "Region for Cloud Run, Firestore and Artifact Registry. us-central1 supports Cloud Run domain mappings."
  type        = string
  default     = "us-central1"
}

variable "service_name" {
  type    = string
  default = "prodeck"
}

variable "github_repo" {
  description = "owner/repo allowed to deploy via Workload Identity Federation."
  type        = string
  default     = "VNerdIO/ProDeck"
}

variable "domain" {
  description = "Custom domain (e.g. deck.example.com). Leave empty to use only the run.app URL. Verify the domain in Google Search Console first."
  type        = string
  default     = ""
}

variable "bootstrap_admin_email" {
  description = "Email of the first admin, created on first start when no users exist."
  type        = string
}

variable "deletion_protection" {
  description = "Block terraform destroy of the Cloud Run service. Turn on for production."
  type        = bool
  default     = false
}

variable "wif_pool_id" {
  description = "Workload Identity pool ID. GCP reserves deleted pool IDs for 30 days, so change this if a previous pool was deleted."
  type        = string
  default     = "prodeck-github"
}

variable "max_instances" {
  type    = number
  default = 3
}
