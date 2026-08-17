#!/usr/bin/env ruby
# frozen_string_literal: true

require "fileutils"
require "json"
require "minitest/autorun"
require "open3"
require "tmpdir"

ROOT = File.expand_path("..", __dir__)
SCRIPT = File.join(ROOT, "scripts/publish_release.sh")
MOCK_GH = File.join(ROOT, "scripts/test_support/mock_gh.rb")
REPOSITORY = "Tinkora/mcp_doctor"
RELEASE_TAG = "v0.1.6"
RELEASE_SHA = "a" * 40
OTHER_SHA = "c" * 40
TAG_OBJECT_SHA = "b" * 40

class PublishReleaseTest < Minitest::Test
  def setup
    @temporary_directory = Dir.mktmpdir("mcp-doctor-release-test")
    @state_path = File.join(@temporary_directory, "state.json")
    @output_path = File.join(@temporary_directory, "github-output")
    @asset_directory = File.join(@temporary_directory, "release-assets")
    @bin_directory = File.join(@temporary_directory, "bin")
    FileUtils.mkdir_p([@asset_directory, @bin_directory])
    File.write(File.join(@asset_directory, "mcp-doctor.tar.gz"), "unix archive")
    File.write(File.join(@asset_directory, "mcp-doctor.zip"), "windows archive")
    FileUtils.cp(MOCK_GH, File.join(@bin_directory, "gh"))
    FileUtils.chmod(0o755, File.join(@bin_directory, "gh"))
    write_state(base_state)
  end

  def teardown
    FileUtils.remove_entry(@temporary_directory)
  end

  def test_create_peels_the_remote_tag_and_uploads_an_owned_draft
    _stdout, stderr, status = run_script("create")

    assert status.success?, stderr
    release = state.fetch("releases").fetch(0)
    assert_equal RELEASE_SHA, release.fetch("target_commitish")
    assert_includes release.fetch("body"), owner_marker
    assert_includes release.fetch("body"), run_marker
    assert_equal %w[mcp-doctor.tar.gz mcp-doctor.zip],
                 release.fetch("assets").map { |asset| asset.fetch("name") }.sort
    assert_equal "release_id=#{release.fetch('id')}\n", File.read(@output_path)
    assert calls.any? { |call| call.join(" ").include?("/git/tags/#{TAG_OBJECT_SHA}") }
  end

  def test_create_replaces_only_an_owned_interrupted_draft
    previous = owned_release(id: 7, body: "#{owner_marker}\n<!-- tinkora-release-run:7:1 -->")
    update_state { |value| value["releases"] << previous }

    _stdout, stderr, status = run_script("create")

    assert status.success?, stderr
    assert_equal [100], state.fetch("releases").map { |release| release.fetch("id") }
    assert calls.any? { |call| delete_call?(call, 7) }
  end

  def test_create_rechecks_draft_ownership_immediately_before_deletion
    previous = owned_release(id: 7, body: "#{owner_marker}\n<!-- tinkora-release-run:7:1 -->")
    update_state do |value|
      value["releases"] << previous
      value["mutate_release_after_first_list"] = true
    end

    _stdout, stderr, status = run_script("create")

    refute status.success?
    assert_includes stderr, "changed before replacement"
    assert_equal [7], state.fetch("releases").map { |release| release.fetch("id") }
    refute calls.any? { |call| delete_call?(call, 7) }
  end

  def test_create_refuses_an_unowned_draft_without_deleting_it
    update_state do |value|
      value["releases"] << owned_release(id: 8, body: "Created manually")
    end

    _stdout, stderr, status = run_script("create")

    refute status.success?
    assert_includes stderr, "not owned by this release workflow"
    assert_equal [8], state.fetch("releases").map { |release| release.fetch("id") }
    refute calls.any? { |call| delete_call?(call, 8) }
  end

  def test_create_refuses_a_published_release_without_deleting_it
    published = owned_release(id: 9, body: owner_marker)
    published["draft"] = false
    update_state { |value| value["releases"] << published }

    _stdout, stderr, status = run_script("create")

    refute status.success?
    assert_includes stderr, "refusing to overwrite published release"
    assert_equal [9], state.fetch("releases").map { |release| release.fetch("id") }
    refute calls.any? { |call| delete_call?(call, 9) }
  end

  def test_cleanup_without_an_id_never_deletes_an_unowned_draft
    update_state do |value|
      value["releases"] << owned_release(id: 10, body: "Created manually")
    end

    _stdout, stderr, status = run_script("cleanup", "")

    assert status.success?, stderr
    assert_equal [10], state.fetch("releases").map { |release| release.fetch("id") }
    refute calls.any? { |call| delete_call?(call, 10) }
  end

  def test_cleanup_finds_and_deletes_only_the_current_run_draft
    update_state { |value| value["create_failure"] = "after_create" }
    _stdout, _stderr, create_status = run_script("create")
    refute create_status.success?

    _stdout, stderr, cleanup_status = run_script("cleanup", "")

    assert cleanup_status.success?, stderr
    assert_empty state.fetch("releases")
    assert calls.any? { |call| delete_call?(call, 100) }
  end

  def test_publish_rejects_a_remote_tag_move_after_draft_creation
    release_id = create_release
    update_state do |value|
      value.fetch("tag_objects").fetch(TAG_OBJECT_SHA)["sha"] = OTHER_SHA
    end

    _stdout, stderr, status = run_script("publish", release_id)

    refute status.success?
    assert_includes stderr, "remote release tag does not identify the workflow commit"
    assert state.fetch("releases").fetch(0).fetch("draft")
    refute calls.any? { |call| patch_call?(call, release_id) }
  end

  def test_publish_rejects_changed_remote_asset_digests
    release_id = create_release
    update_state do |value|
      value.fetch("releases").fetch(0).fetch("assets").fetch(0)["digest"] = "sha256:changed"
    end

    _stdout, stderr, status = run_script("publish", release_id)

    refute status.success?
    assert_includes stderr, "Remote release asset digest does not match"
    assert state.fetch("releases").fetch(0).fetch("draft")
    refute calls.any? { |call| patch_call?(call, release_id) }
  end

  def test_publish_rechecks_assets_after_the_final_tag_verification
    release_id = create_release
    update_state { |value| value["mutate_release_after_first_read"] = true }

    _stdout, stderr, status = run_script("publish", release_id)

    refute status.success?
    assert_includes stderr, "Remote release asset digest does not match"
    assert state.fetch("releases").fetch(0).fetch("draft")
    assert_equal 2, state.fetch("release_reads")
    refute calls.any? { |call| patch_call?(call, release_id) }
  end

  def test_publish_verifies_the_current_run_assets_before_patching_by_id
    release_id = create_release

    _stdout, stderr, status = run_script("publish", release_id)

    assert status.success?, stderr
    refute state.fetch("releases").fetch(0).fetch("draft")
    assert calls.any? { |call| patch_call?(call, release_id) }
  end

  private

  def base_state
    {
      "tag_ref" => { "type" => "tag", "sha" => TAG_OBJECT_SHA },
      "tag_objects" => {
        TAG_OBJECT_SHA => { "type" => "commit", "sha" => RELEASE_SHA }
      },
      "releases" => [],
      "next_id" => 100,
      "calls" => []
    }
  end

  def owned_release(id:, body:)
    {
      "id" => id,
      "tag_name" => RELEASE_TAG,
      "target_commitish" => RELEASE_SHA,
      "draft" => true,
      "prerelease" => false,
      "body" => body,
      "assets" => []
    }
  end

  def owner_marker
    "<!-- tinkora-release-owner:#{REPOSITORY}:#{RELEASE_TAG}:#{RELEASE_SHA} -->"
  end

  def run_marker
    "<!-- tinkora-release-run:42:1 -->"
  end

  def run_script(mode, release_id = nil)
    File.write(@output_path, "") if mode == "create"
    arguments = ["bash", SCRIPT, mode, @asset_directory]
    arguments << release_id.to_s unless release_id.nil?
    Open3.capture3(
      {
        "PATH" => "#{@bin_directory}:#{ENV.fetch('PATH')}",
        "MOCK_GH_STATE" => @state_path,
        "GH_REPO" => REPOSITORY,
        "RELEASE_TAG" => RELEASE_TAG,
        "GITHUB_SHA" => RELEASE_SHA,
        "GITHUB_RUN_ID" => "42",
        "GITHUB_RUN_ATTEMPT" => "1",
        "GITHUB_OUTPUT" => @output_path
      },
      *arguments,
      chdir: ROOT
    )
  end

  def create_release
    _stdout, stderr, status = run_script("create")
    assert status.success?, stderr
    state.fetch("releases").fetch(0).fetch("id")
  end

  def state
    JSON.parse(File.read(@state_path, encoding: "UTF-8"))
  end

  def calls
    state.fetch("calls")
  end

  def write_state(value)
    File.write(@state_path, JSON.pretty_generate(value), encoding: "UTF-8")
  end

  def update_state
    value = state
    yield value
    write_state(value)
  end

  def delete_call?(call, release_id)
    call.include?("DELETE") && call.any? { |argument| argument.end_with?("/releases/#{release_id}") }
  end

  def patch_call?(call, release_id)
    call.include?("PATCH") && call.any? { |argument| argument.end_with?("/releases/#{release_id}") }
  end
end
