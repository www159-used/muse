Feature: Select NCM sources in download folders

  Scenario: Importing a mixed download folder selects only NCM files
    Given "downloads/song.ncm" is a copy of the NCM fixture "standard.ncm"
    And "downloads/plain.mp3" is a copy of the fixture "tagged.mp3"
    And "downloads/plain.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "silence.mp3"
