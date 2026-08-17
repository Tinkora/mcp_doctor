#!/usr/bin/env ruby
# frozen_string_literal: true

require "digest"
require "json"

state_path = ENV.fetch("MOCK_GH_STATE")
state = JSON.parse(File.read(state_path, encoding: "UTF-8"))
arguments = ARGV.dup
state["calls"] << arguments.dup

at_exit do
  File.write(state_path, JSON.pretty_generate(state), encoding: "UTF-8")
end

def parse_api_arguments(arguments)
  method = "GET"
  endpoint = nil
  fields = {}
  slurp = false
  until arguments.empty?
    argument = arguments.shift
    case argument
    when "--method"
      method = arguments.shift
    when "--paginate"
      nil
    when "--slurp"
      slurp = true
    when "--jq"
      arguments.shift
    when "-F", "-f"
      key, value = arguments.shift.split("=", 2)
      fields[key] = value
    else
      endpoint ||= argument
    end
  end
  [method, endpoint, fields, slurp]
end

def converted_field(value)
  case value
  when "true" then true
  when "false" then false
  else value
  end
end

command = arguments.shift
case command
when "api"
  method, endpoint, fields, slurp = parse_api_arguments(arguments)
  case [method, endpoint]
  in ["GET", %r{/git/ref/tags/}]
    puts JSON.generate("object" => state.fetch("tag_ref"))
  in ["GET", %r{/git/tags/([^/?]+)}]
    tag_sha = Regexp.last_match(1)
    puts JSON.generate("object" => state.fetch("tag_objects").fetch(tag_sha))
  in ["GET", %r{/releases\?per_page=100$}]
    payload = state.fetch("releases")
    serialized_payload = JSON.generate(slurp ? [payload] : payload)
    state["release_list_reads"] = state.fetch("release_list_reads", 0) + 1
    if state["mutate_release_after_first_list"] && state.fetch("release_list_reads") == 1
      state.fetch("releases").fetch(0)["body"] = "Created manually after listing"
    end
    puts serialized_payload
  in ["GET", %r{/releases/(\d+)$}]
    release_id = Regexp.last_match(1).to_i
    release = state.fetch("releases").find { |item| item.fetch("id") == release_id }
    abort("release #{release_id} not found") unless release
    payload = JSON.generate(release)
    state["release_reads"] = state.fetch("release_reads", 0) + 1
    if state["mutate_release_after_first_read"] && state.fetch("release_reads") == 1
      release.fetch("assets").fetch(0)["digest"] = "sha256:changed-after-read"
    end
    puts payload
  in ["DELETE", %r{/releases/(\d+)$}]
    release_id = Regexp.last_match(1).to_i
    state.fetch("releases").reject! { |item| item.fetch("id") == release_id }
  in ["PATCH", %r{/releases/(\d+)$}]
    release_id = Regexp.last_match(1).to_i
    release = state.fetch("releases").find { |item| item.fetch("id") == release_id }
    abort("release #{release_id} not found") unless release
    fields.each { |key, value| release[key] = converted_field(value) }
    puts JSON.generate(release)
  else
    abort("unsupported gh api invocation: #{method} #{endpoint}")
  end
when "release"
  abort("only gh release create is supported") unless arguments.shift == "create"

  tag = arguments.shift
  assets = []
  assets << arguments.shift while arguments.first && !arguments.first.start_with?("--")
  options = {}
  until arguments.empty?
    argument = arguments.shift
    case argument
    when "--repo", "--target", "--title", "--notes"
      options[argument] = arguments.shift
    when "--verify-tag", "--generate-notes", "--draft"
      options[argument] = true
    else
      abort("unsupported gh release create option: #{argument}")
    end
  end

  release_id = state.fetch("next_id")
  state["next_id"] = release_id + 1
  release = {
    "id" => release_id,
    "tag_name" => tag,
    "target_commitish" => options.fetch("--target"),
    "draft" => true,
    "prerelease" => false,
    "body" => options.fetch("--notes"),
    "assets" => assets.map do |path|
      {
        "name" => File.basename(path),
        "digest" => "sha256:#{Digest::SHA256.file(path).hexdigest}"
      }
    end
  }
  state.fetch("releases") << release
  abort("simulated upload failure") if state["create_failure"] == "after_create"

  puts "https://example.test/releases/#{release_id}"
else
  abort("unsupported gh command: #{command}")
end
