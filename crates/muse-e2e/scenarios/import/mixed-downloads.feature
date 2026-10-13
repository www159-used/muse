Feature: Import mixed download folders

  Scenario: Importing a mixed download folder includes ordinary MP3 and FLAC
    Given "downloads/song.ncm" is a copy of the NCM fixture "standard.ncm"
    And "downloads/plain.mp3" is a copy of the fixture "tagged.mp3"
    And "downloads/plain.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    Then 3 tracks are added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title         | audio       |
      | Fixture NCM   | silence.mp3 |
      | Changed title | tagged.mp3  |
      | plain         | tone.flac   |
