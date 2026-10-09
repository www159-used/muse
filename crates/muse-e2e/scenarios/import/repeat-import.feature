Feature: Import NCM downloads

  Scenario: Importing a folder twice skips duplicate audio
    Given "downloads/song.ncm" is a copy of the NCM fixture "standard.ncm"

    When I import the folder "downloads"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "silence.mp3"

    When I import the folder "downloads"
    Then 0 tracks are added
    And 1 duplicate is skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "silence.mp3"
