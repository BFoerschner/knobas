# The payout bucket.
terraform {
  required_version = ">= 1.6"
}

variable "retries" {
  type    = number
  default = 3
}

resource "aws_s3_bucket" "payouts" {
  bucket = "payouts-${var.env}"
  tags = {
    Owner   = "payments"
    Enabled = true
  }
}
