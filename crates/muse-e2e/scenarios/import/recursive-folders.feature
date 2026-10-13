Feature: Import nested download folders

  Scenario: Importing a download root finds songs in artist and album folders
    Given "downloads/Artist One/Album One/song.ncm" is a copy of the NCM fixture "standard.ncm"
    And "downloads/Artist Two/Album Two/Disc 1/lossless.ncm" is a copy of the NCM fixture "other-artist-flac.ncm"

    When I import the folder "downloads"
    Then 2 tracks are added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio       |
      | Fixture NCM | silence.mp3 |
      | Fixture NCM | tone.flac   |
