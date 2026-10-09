Feature: Import FLAC downloads

  Scenario: Importing a FLAC download keeps the unpacked FLAC audio
    Given "downloads/lossless.ncm" is a copy of the NCM fixture "flac.ncm"

    When I import the folder "downloads"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "tone.flac"
    And the imported track has artist "Fixture Artist", album "Fixture Album" and NetEase ID 321
    And the imported track has a PNG cover matching "cover.png"
