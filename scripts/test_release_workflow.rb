#!/usr/bin/env ruby
# frozen_string_literal: true

workflow = File.read(
  File.expand_path("../.github/workflows/release.yml", __dir__),
  encoding: "UTF-8"
)

required_fragments = [
  "cargo-cyclonedx --version 0.5.9 --locked",
  "mcp-doctor-${RELEASE_TAG}.cdx.json",
  "actions/attest-sbom@4651f806c01d8637787e274ac3bdf724ef169f34",
  'release_id="$(gh api',
  'repos/${GH_REPO}/releases/${release_id}',
  "-F draft=false"
]

required_fragments.each do |fragment|
  abort("release workflow is missing #{fragment.inspect}") unless workflow.include?(fragment)
end

abort("draft releases must be published by release id") if workflow.include?("gh release edit")

puts "release workflow contract passed"
