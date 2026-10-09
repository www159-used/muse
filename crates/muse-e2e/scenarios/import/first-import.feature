Feature: Import NCM audio and metadata

  Scenario: Importing a new MP3 download preserves its metadata and cover
    Given "downloads/song.ncm" is a copy of the NCM fixture "standard.ncm"

    When I import the folder "downloads"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "silence.mp3"
    And the imported track has artist "Fixture Artist", album "Fixture Album" and NetEase ID 321
    And the imported track has a PNG cover matching "cover.png"
