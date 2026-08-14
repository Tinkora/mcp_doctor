#!/usr/bin/env ruby
# frozen_string_literal: true

workflow = File.read(
  File.expand_path("../.github/workflows/release.yml", __dir__),
  encoding: "UTF-8"
)

required_fragments = [
  "cargo-cyclonedx --version 0.5.9 --locked",
  "mcp-doctor-${RELEASE_TAG}.cdx.json",
  "actions/attest@",
  "subject-path:",
  "sbom-path:",
  'release_id="$(gh api',
  'repos/${GH_REPO}/releases/${release_id}',
  "-F draft=false"
]

required_fragments.each do |fragment|
  abort("release workflow is missing #{fragment.inspect}") unless workflow.include?(fragment)
end

action_references = workflow.scan(/^\s*uses:\s*([^@\s]+)@([^\s#]+)/).map do |name, revision|
  [name, revision]
end
unpinned_actions = action_references.reject { |_name, revision| revision.match?(/\A[0-9a-f]{40}\z/) }
unless unpinned_actions.empty?
  abort("release workflow has unpinned actions: #{unpinned_actions.map(&:first).join(', ')}")
end

abort("draft releases must be published by release id") if workflow.include?("gh release edit")

puts "release workflow contract passed"
