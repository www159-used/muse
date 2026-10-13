Feature: Import multiple downloads

  Scenario: Importing a folder adds each distinct audio file
    Given "downloads/song.ncm" is a copy of the NCM fixture "standard.ncm"
    And "downloads/lossless.ncm" is a copy of the NCM fixture "other-artist-flac.ncm"

    When I import the folder "downloads"
    Then 2 tracks are added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio       |
      | Fixture NCM | tone.flac   |
      | Fixture NCM | silence.mp3 |
