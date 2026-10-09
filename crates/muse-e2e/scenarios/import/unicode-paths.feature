Feature: Import downloads from Unicode paths

  Scenario: Importing a folder with Chinese and Korean names and spaces
    Given "网易云音乐 下载/한국어 노래/夜空 노래.NCM" is a copy of the NCM fixture "standard.ncm"

    When I import the folder "网易云音乐 下载"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "silence.mp3"
